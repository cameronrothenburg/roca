//! Embedded V8 runtime (via deno_core) — executes compiled JS without external dependencies.
//! Web APIs (URL, TextEncoder, etc.) are provided via polyfills and custom Rust ops.
//! Compiled JS output uses globalThis APIs — works in any JS environment.

use deno_core::{JsRuntime, RuntimeOptions, op2, extension};
use std::cell::RefCell;
use std::sync::LazyLock;

const BOOTSTRAP: &str = include_str!("../../packages/runtime/bootstrap.js");
const POLYFILLS: &str = include_str!("../../packages/runtime/polyfills.js");
const URL_BRIDGE: &str = include_str!("../../packages/runtime/url-bridge.js");
const CRYPTO_BRIDGE: &str = include_str!("../../packages/runtime/crypto-bridge.js");
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

#[op2(fast)]
fn op_capture_log(#[string] msg: &str) {
    CAPTURED.with(|c| c.borrow_mut().push(msg.to_string()));
}

/// Parse a URL, return JSON with components or empty string on failure.
#[op2]
#[string]
fn op_url_parse(#[string] raw: &str) -> String {
    match url::Url::parse(raw) {
        Ok(u) => serde_json::json!({
            "href": u.as_str(),
            "origin": u.origin().ascii_serialization(),
            "protocol": format!("{}:", u.scheme()),
            "hostname": u.host_str().unwrap_or(""),
            "host": format!("{}{}", u.host_str().unwrap_or(""), u.port().map(|p| format!(":{}", p)).unwrap_or_default()),
            "port": u.port().map(|p| p.to_string()).unwrap_or_default(),
            "pathname": u.path(),
            "search": u.query().map(|q| format!("?{}", q)).unwrap_or_default(),
            "hash": u.fragment().map(|f| format!("#{}", f)).unwrap_or_default(),
        }).to_string(),
        Err(_) => String::new(),
    }
}

/// SHA-256 hash, returns hex string.
#[op2]
#[string]
fn op_sha256(#[string] data: &str) -> String {
    use sha2::{Sha256, Digest};
    hex_encode(&Sha256::digest(data.as_bytes()))
}

/// SHA-512 hash, returns hex string.
#[op2]
#[string]
fn op_sha512(#[string] data: &str) -> String {
    use sha2::{Sha512, Digest};
    hex_encode(&Sha512::digest(data.as_bytes()))
}

fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(hex, "{:02x}", b);
    }
    hex
}

/// Generate a v4 UUID.
#[op2]
#[string]
fn op_random_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}

extension!(
    roca_runtime,
    ops = [op_capture_log, op_url_parse, op_sha256, op_sha512, op_random_uuid],
);

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
    runtime.execute_script("<crypto-bridge>", CRYPTO_BRIDGE.to_string())
        .map_err(|e| format!("crypto bridge error: {}", e))?;
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
