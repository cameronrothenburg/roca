//! Embedded V8 runtime (via deno_core) — executes compiled JS without external dependencies.

use deno_core::{JsRuntime, RuntimeOptions, op2, extension};
use std::cell::RefCell;

const BOOTSTRAP_RUN: &str = include_str!("../../packages/runtime/bootstrap-run.js");
const BOOTSTRAP_TEST: &str = include_str!("../../packages/runtime/bootstrap-test.js");
const POLYFILLS: &str = include_str!("../../packages/runtime/polyfills.js");
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

/// Execute JS and stream stdout/stderr directly (for `roca run`).
pub fn run_js(code: &str) -> bool {
    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![roca_runtime::ext()],
        ..Default::default()
    });

    if let Err(e) = runtime.execute_script("<bootstrap>", BOOTSTRAP_RUN.to_string()) {
        eprintln!("bootstrap error: {}", e);
        return false;
    }
    let _ = runtime.execute_script("<polyfills>", POLYFILLS.to_string());

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
    CAPTURED.with(|c| c.borrow_mut().clear());

    let mut runtime = JsRuntime::new(RuntimeOptions {
        extensions: vec![roca_runtime::ext()],
        ..Default::default()
    });

    if let Err(e) = runtime.execute_script("<bootstrap>", BOOTSTRAP_TEST.to_string()) {
        return (format!("bootstrap error: {}\n", e), false);
    }
    let _ = runtime.execute_script("<polyfills>", POLYFILLS.to_string());

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
