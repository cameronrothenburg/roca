//! Embedded QuickJS runtime — executes compiled JS without external dependencies.

use rquickjs::{Context, Runtime, Function, function::Rest};

const ROCA_TEST_JS: &str = include_str!("../../packages/stdlib/roca-test.js");

fn setup_context(ctx: &rquickjs::Ctx, captured: Option<std::sync::Arc<std::sync::Mutex<String>>>) {
    let globals = ctx.globals();
    let console = rquickjs::Object::new(ctx.clone()).unwrap();

    if let Some(cap) = captured.clone() {
        let cap2 = cap.clone();
        let log = Function::new(ctx.clone(), move |args: Rest<rquickjs::Value>| {
            let parts: Vec<String> = args.0.iter().map(|v| js_value_to_string(v)).collect();
            let line = parts.join(" ");
            let mut buf = cap.lock().unwrap();
            buf.push_str(&line);
            buf.push('\n');
        }).unwrap();
        console.set("log", log).unwrap();

        let error_fn = Function::new(ctx.clone(), move |args: Rest<rquickjs::Value>| {
            let parts: Vec<String> = args.0.iter().map(|v| js_value_to_string(v)).collect();
            let line = parts.join(" ");
            let mut buf = cap2.lock().unwrap();
            buf.push_str(&line);
            buf.push('\n');
        }).unwrap();
        console.set("error", error_fn).unwrap();
    } else {
        let log = Function::new(ctx.clone(), |args: Rest<rquickjs::Value>| {
            let parts: Vec<String> = args.0.iter().map(|v| js_value_to_string(v)).collect();
            println!("{}", parts.join(" "));
        }).unwrap();
        console.set("log", log).unwrap();

        let error_fn = Function::new(ctx.clone(), |args: Rest<rquickjs::Value>| {
            let parts: Vec<String> = args.0.iter().map(|v| js_value_to_string(v)).collect();
            eprintln!("{}", parts.join(" "));
        }).unwrap();
        console.set("error", error_fn).unwrap();
    }

    let warn_fn = Function::new(ctx.clone(), |args: Rest<rquickjs::Value>| {
        let parts: Vec<String> = args.0.iter().map(|v| js_value_to_string(v)).collect();
        eprintln!("{}", parts.join(" "));
    }).unwrap();
    console.set("warn", warn_fn).unwrap();
    globals.set("console", console).unwrap();

    // process.exit — throw to abort execution
    let process = rquickjs::Object::new(ctx.clone()).unwrap();
    let exit_fn = Function::new(ctx.clone(), |code: i32| -> rquickjs::Result<()> {
        if code != 0 {
            Err(rquickjs::Error::Exception)
        } else {
            Ok(())
        }
    }).unwrap();
    process.set("exit", exit_fn).unwrap();
    globals.set("process", process).unwrap();
}

/// Execute JS and stream stdout/stderr directly (for `roca run`).
pub fn run_js(code: &str) -> bool {
    let rt = Runtime::new().expect("failed to create QuickJS runtime");
    let ctx = Context::full(&rt).expect("failed to create QuickJS context");
    let code = code.to_string();
    let mut success = true;

    ctx.with(|ctx| {
        setup_context(&ctx, None);
        match ctx.eval::<rquickjs::Value, _>(code.as_str()) {
            Ok(_) => {}
            Err(e) => {
                report_error(&ctx, &e);
                success = false;
            }
        }
    });

    // Drain promise queue
    while rt.execute_pending_job().unwrap_or(false) {}

    success
}

/// Execute test JS, capturing output for parsing. Returns (output, success).
pub fn run_tests(code: &str) -> (String, bool) {
    let rt = Runtime::new().expect("failed to create QuickJS runtime");
    let ctx = Context::full(&rt).expect("failed to create QuickJS context");

    // Inject roca-test.js as globals so battle tests work without require()
    let code = format!("{}\n{}", inject_test_runtime(), code);
    let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let mut success = true;

    ctx.with(|ctx| {
        setup_context(&ctx, Some(captured.clone()));
        match ctx.eval::<rquickjs::Value, _>(code.as_str()) {
            Ok(_) => {}
            Err(e) => {
                report_error(&ctx, &e);
                success = false;
            }
        }
    });

    // Drain the promise/job queue so async tests complete
    while rt.execute_pending_job().unwrap_or(false) {}

    let output = captured.lock().unwrap().clone();
    (output, success)
}

/// Wrap the roca-test.js content so it exposes fc, battleTest, arb as globals.
fn inject_test_runtime() -> String {
    format!(
        "var fc, battleTest, arb;\ntry {{\nvar module = {{ exports: {{}} }};\n{}\nfc = module.exports.fc;\nbattleTest = module.exports.battleTest;\narb = module.exports.arb;\n}} catch(_e) {{}}\n",
        ROCA_TEST_JS
    )
}

fn report_error(ctx: &rquickjs::Ctx, e: &rquickjs::Error) {
    if let rquickjs::Error::Exception = e {
        let caught = ctx.catch();
        if let Some(exc) = caught.as_exception() {
            let msg = exc.message().unwrap_or_default();
            let stack = exc.stack().unwrap_or_default();
            eprintln!("runtime error: {}", msg);
            if !stack.is_empty() { eprintln!("{}", stack); }
        }
    } else {
        eprintln!("runtime error: {}", e);
    }
}

fn js_value_to_string(val: &rquickjs::Value) -> String {
    if let Some(s) = val.as_string() {
        s.to_string().unwrap_or_default()
    } else if let Some(n) = val.as_int() {
        n.to_string()
    } else if let Some(n) = val.as_float() {
        if n.fract() == 0.0 && n.abs() < 1e15 {
            format!("{}", n as i64)
        } else {
            format!("{}", n)
        }
    } else if let Some(b) = val.as_bool() {
        b.to_string()
    } else if val.is_null() {
        "null".to_string()
    } else if val.is_undefined() {
        "undefined".to_string()
    } else {
        "[object]".to_string()
    }
}
