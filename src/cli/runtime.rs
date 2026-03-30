//! Embedded V8 runtime (via deno_core) — executes compiled JS without external dependencies.

use deno_core::{JsRuntime, RuntimeOptions, op2, extension};
use std::cell::RefCell;

const ROCA_TEST_JS: &str = include_str!("../../packages/stdlib/roca-test.js");

thread_local! {
    static CAPTURED: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

#[op2(fast)]
fn op_capture_log(#[string] msg: &str) {
    CAPTURED.with(|c| c.borrow_mut().push(msg.to_string()));
}

extension!(
    roca_runtime,
    ops = [op_capture_log],
);

/// Bootstrap JS for direct output mode (roca run)
const RUN_BOOTSTRAP: &str = r#"
globalThis.console = {
    log: (...args) => {
        const msg = args.map(a => typeof a === 'string' ? a : JSON.stringify(a)).join(' ');
        Deno.core.print(msg + '\n', false);
    },
    error: (...args) => {
        const msg = args.map(a => typeof a === 'string' ? a : JSON.stringify(a)).join(' ');
        Deno.core.print(msg + '\n', true);
    },
    warn: (...args) => {
        const msg = args.map(a => typeof a === 'string' ? a : JSON.stringify(a)).join(' ');
        Deno.core.print(msg + '\n', true);
    },
};
globalThis.process = {
    exit: (code) => { if (code !== 0) throw new Error('__PROCESS_EXIT__'); },
};
"#;

/// Web API polyfills for V8 (TextEncoder, TextDecoder, atob, btoa)
const WEB_POLYFILLS: &str = r#"
if (typeof TextEncoder === 'undefined') {
    globalThis.TextEncoder = class TextEncoder {
        encode(str) {
            const buf = [];
            for (let i = 0; i < str.length; i++) {
                let c = str.charCodeAt(i);
                if (c < 0x80) buf.push(c);
                else if (c < 0x800) { buf.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f)); }
                else { buf.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f)); }
            }
            return new Uint8Array(buf);
        }
    };
}
if (typeof TextDecoder === 'undefined') {
    globalThis.TextDecoder = class TextDecoder {
        decode(buf) {
            const bytes = new Uint8Array(buf);
            let str = '', i = 0;
            while (i < bytes.length) {
                let c = bytes[i];
                if (c < 0x80) { str += String.fromCharCode(c); i++; }
                else if (c < 0xe0) { str += String.fromCharCode(((c & 0x1f) << 6) | (bytes[i+1] & 0x3f)); i += 2; }
                else { str += String.fromCharCode(((c & 0x0f) << 12) | ((bytes[i+1] & 0x3f) << 6) | (bytes[i+2] & 0x3f)); i += 3; }
            }
            return str;
        }
    };
}
if (typeof atob === 'undefined') {
    const b64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/=';
    globalThis.atob = (s) => {
        let r = '', i = 0;
        s = s.replace(/[^A-Za-z0-9+/=]/g, '');
        while (i < s.length) {
            const a = b64.indexOf(s[i++]), b = b64.indexOf(s[i++]);
            const c = b64.indexOf(s[i++]), d = b64.indexOf(s[i++]);
            r += String.fromCharCode((a << 2) | (b >> 4));
            if (c !== 64) r += String.fromCharCode(((b & 15) << 4) | (c >> 2));
            if (d !== 64) r += String.fromCharCode(((c & 3) << 6) | d);
        }
        return r;
    };
    globalThis.btoa = (s) => {
        let r = '', i = 0;
        while (i < s.length) {
            const a = s.charCodeAt(i++), b = i < s.length ? s.charCodeAt(i++) : NaN;
            const c = i < s.length ? s.charCodeAt(i++) : NaN;
            r += b64[a >> 2] + b64[((a & 3) << 4) | (b >> 4)];
            r += isNaN(b) ? '==' : b64[((b & 15) << 2) | (c >> 6)] + (isNaN(c) ? '=' : b64[c & 63]);
        }
        return r;
    };
}
if (typeof setTimeout === 'undefined') {
    globalThis.setTimeout = (fn, ms) => { Promise.resolve().then(fn); return 0; };
    globalThis.clearTimeout = () => {};
}
"#;

/// Bootstrap JS for test capture mode (roca build)
const TEST_BOOTSTRAP: &str = r#"
globalThis.console = {
    log: (...args) => {
        const msg = args.map(a => typeof a === 'string' ? a : JSON.stringify(a)).join(' ');
        Deno.core.ops.op_capture_log(msg);
    },
    error: (...args) => {
        const msg = args.map(a => typeof a === 'string' ? a : JSON.stringify(a)).join(' ');
        Deno.core.print(msg + '\n', true);
    },
    warn: (...args) => {
        const msg = args.map(a => typeof a === 'string' ? a : JSON.stringify(a)).join(' ');
        Deno.core.print(msg + '\n', true);
    },
};
globalThis.process = {
    exit: (code) => { if (code !== 0) throw new Error('__PROCESS_EXIT__'); },
};
"#;

/// Execute JS and stream stdout/stderr directly (for `roca run`).
pub fn run_js(code: &str) -> bool {
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![roca_runtime::ext()],
        ..Default::default()
    });

    if let Err(e) = runtime.execute_script("<bootstrap>", RUN_BOOTSTRAP.to_string()) {
        eprintln!("bootstrap error: {}", e);
        return false;
    }
    let _ = runtime.execute_script("<polyfills>", WEB_POLYFILLS.to_string());

    match runtime.execute_script("<roca>", code.to_string()) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("runtime error: {}", e);
            return false;
        }
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let _ = runtime.run_event_loop(Default::default()).await;
    });

    true
}

/// Execute test JS, capturing console.log output for parsing. Returns (output, success).
pub fn run_tests(code: &str) -> (String, bool) {
    // Clear captured output
    CAPTURED.with(|c| c.borrow_mut().clear());

    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![roca_runtime::ext()],
        ..Default::default()
    });

    if let Err(e) = runtime.execute_script("<bootstrap>", TEST_BOOTSTRAP.to_string()) {
        return (format!("bootstrap error: {}\n", e), false);
    }
    let _ = runtime.execute_script("<polyfills>", WEB_POLYFILLS.to_string());

    // Inject roca-test.js globals
    if let Err(e) = runtime.execute_script("<roca-test>", inject_test_runtime()) {
        return (format!("test runtime error: {}\n", e), false);
    }

    // Wrap in async IIFE so top-level await works in script mode
    let wrapped = format!("(async () => {{\n{}\n}})();", code);

    let mut success = true;
    match runtime.execute_script("<test>", wrapped) {
        Ok(_) => {}
        Err(e) => {
            let msg = format!("{}", e);
            if !msg.contains("__PROCESS_EXIT__") {
                eprintln!("runtime error: {}", e);
            }
            success = false;
        }
    }

    // Drain async jobs (resolves the IIFE promise)
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        if let Err(e) = runtime.run_event_loop(Default::default()).await {
            let msg = format!("{}", e);
            if !msg.contains("__PROCESS_EXIT__") {
                eprintln!("runtime error: {}", e);
            }
            success = false;
        }
    });

    let output = CAPTURED.with(|c| {
        let lines = c.borrow();
        if lines.is_empty() {
            String::new()
        } else {
            lines.join("\n") + "\n"
        }
    });

    (output, success)
}

fn inject_test_runtime() -> String {
    format!(
        "var fc, battleTest, arb;\ntry {{\nvar module = {{ exports: {{}} }};\n(function() {{\n{}\n}})();\nfc = module.exports.fc;\nbattleTest = module.exports.battleTest;\narb = module.exports.arb;\n}} catch(_e) {{}}\n",
        ROCA_TEST_JS
    )
}
