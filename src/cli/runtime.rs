//! Embedded V8 runtime (via deno_core) — executes compiled JS without external dependencies.
//! Web APIs (URL, TextEncoder, etc.) are provided via custom Rust ops, not deno extension crates.

use deno_core::{JsRuntime, RuntimeOptions, op2, extension};
use std::cell::RefCell;
use std::sync::LazyLock;

const BOOTSTRAP: &str = include_str!("../../packages/runtime/bootstrap.js");
const POLYFILLS: &str = include_str!("../../packages/runtime/polyfills.js");
const ROCA_TEST_JS: &str = include_str!("../../packages/stdlib/roca-test.js");

const PROCESS_EXIT_SENTINEL: &str = "__PROCESS_EXIT__";

static TEST_RUNTIME_JS: LazyLock<String> = LazyLock::new(|| {
    format!(
        "var fc, battleTest, arb;\ntry {{\nvar module = {{ exports: {{}} }};\n(function() {{\n{}\n}})();\nfc = module.exports.fc;\nbattleTest = module.exports.battleTest;\narb = module.exports.arb;\n}} catch(_e) {{ if (_e) Deno.core.print('warning: roca-test init failed: ' + _e + '\\n', true); }}\n",
        ROCA_TEST_JS
    )
});

thread_local! {
    static CAPTURED: RefCell<Vec<String>> = RefCell::new(Vec::new());
}

// ─── Ops ───────────────────────────────────────────────────

#[op2(fast)]
fn op_capture_log(#[string] msg: &str) {
    CAPTURED.with(|c| c.borrow_mut().push(msg.to_string()));
}

/// Parse a URL string, return JSON with parsed components or empty string on failure.
#[op2]
#[string]
fn op_url_parse(#[string] raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(u) => format!(
            r#"{{"href":"{}","origin":"{}","protocol":"{}","hostname":"{}","host":"{}","port":"{}","pathname":"{}","search":"{}","hash":"{}"}}"#,
            u.as_str(),
            u.origin().ascii_serialization(),
            u.scheme().to_string() + ":",
            u.host_str().unwrap_or(""),
            u.host_str().unwrap_or("").to_string() + &u.port().map(|p| format!(":{}", p)).unwrap_or_default(),
            u.port().map(|p| p.to_string()).unwrap_or_default(),
            u.path(),
            u.query().map(|q| format!("?{}", q)).unwrap_or_default(),
            u.fragment().map(|f| format!("#{}", f)).unwrap_or_default(),
        ),
        Err(_) => String::new(),
    }
}

extension!(
    roca_runtime,
    ops = [op_capture_log, op_url_parse],
);

// ─── Runtime setup ─────────────────────────────────────────

/// JS wrapper that exposes URL via our Rust op
const URL_BRIDGE: &str = r#"
if (typeof URLSearchParams === 'undefined') {
    globalThis.URLSearchParams = class URLSearchParams {
        constructor(init) {
            this._params = [];
            if (typeof init === 'string') {
                const s = init.startsWith('?') ? init.slice(1) : init;
                if (s) s.split('&').forEach(p => {
                    const [k, ...v] = p.split('=');
                    this._params.push([decodeURIComponent(k), decodeURIComponent(v.join('='))]);
                });
            }
        }
        get(name) { const e = this._params.find(p => p[0] === name); return e ? e[1] : null; }
        has(name) { return this._params.some(p => p[0] === name); }
        get size() { return this._params.length; }
        toString() { return this._params.map(([k,v]) => encodeURIComponent(k) + '=' + encodeURIComponent(v)).join('&'); }
    };
}
globalThis.URL = class URL {
    constructor(raw) {
        const json = Deno.core.ops.op_url_parse(raw);
        if (!json) throw new TypeError("Invalid URL: " + raw);
        const p = JSON.parse(json);
        this.href = p.href;
        this.origin = p.origin;
        this.protocol = p.protocol;
        this.hostname = p.hostname;
        this.host = p.host;
        this.port = p.port;
        this.pathname = p.pathname;
        this.search = p.search;
        this.hash = p.hash;
        this.searchParams = new URLSearchParams(this.search);
    }
    toString() { return this.href; }
};
"#;

fn create_runtime() -> JsRuntime {
    JsRuntime::new(RuntimeOptions {
        extensions: vec![roca_runtime::ext()],
        ..Default::default()
    })
}

fn bootstrap(runtime: &mut JsRuntime, capture: bool) -> Result<(), String> {
    let capture_flag = if capture {
        "var __ROCA_CAPTURE_MODE__ = true;\n"
    } else {
        "var __ROCA_CAPTURE_MODE__ = false;\n"
    };

    runtime.execute_script("<capture-flag>", capture_flag.to_string())
        .map_err(|e| format!("bootstrap error: {}", e))?;
    runtime.execute_script("<bootstrap>", BOOTSTRAP.to_string())
        .map_err(|e| format!("bootstrap error: {}", e))?;
    runtime.execute_script("<polyfills>", POLYFILLS.to_string())
        .map_err(|e| format!("polyfill error: {}", e))?;
    runtime.execute_script("<url-bridge>", URL_BRIDGE.to_string())
        .map_err(|e| format!("url bridge error: {}", e))?;
    Ok(())
}

fn drain_event_loop(runtime: &mut JsRuntime) -> bool {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut success = true;
    rt.block_on(async {
        if let Err(e) = runtime.run_event_loop(Default::default()).await {
            if !is_process_exit(&e) {
                eprintln!("runtime error: {}", e);
            }
            success = false;
        }
    });
    success
}

fn is_process_exit(e: &impl std::fmt::Display) -> bool {
    e.to_string().contains(PROCESS_EXIT_SENTINEL)
}

/// Execute JS and stream stdout/stderr directly (for `roca run`).
pub fn run_js(code: &str) -> bool {
    let mut runtime = create_runtime();

    if let Err(msg) = bootstrap(&mut runtime, false) {
        eprintln!("{}", msg);
        return false;
    }

    match runtime.execute_script("<roca>", code.to_string()) {
        Ok(_) => {}
        Err(e) => {
            eprintln!("runtime error: {}", e);
            return false;
        }
    }

    drain_event_loop(&mut runtime)
}

/// Execute test JS, capturing console.log output for parsing. Returns (output, success).
pub fn run_tests(code: &str) -> (String, bool) {
    CAPTURED.with(|c| c.borrow_mut().clear());

    let mut runtime = create_runtime();

    if let Err(msg) = bootstrap(&mut runtime, true) {
        return (msg + "\n", false);
    }

    if let Err(e) = runtime.execute_script("<roca-test>", TEST_RUNTIME_JS.to_string()) {
        return (format!("test runtime error: {}\n", e), false);
    }

    let wrapped = format!("(async () => {{\n{}\n}})();", code);

    let mut success = true;
    match runtime.execute_script("<test>", wrapped) {
        Ok(_) => {}
        Err(e) => {
            if !is_process_exit(&e) {
                eprintln!("runtime error: {}", e);
            }
            success = false;
        }
    }

    if !drain_event_loop(&mut runtime) {
        success = false;
    }

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
