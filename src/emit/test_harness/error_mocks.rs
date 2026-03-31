//! Auto-generated error mock tests — for each crash `halt` on an error-returning
//! extern contract method, swap the mock to return each declared error and assert
//! the function under test propagates it.

use crate::ast as roca;
use super::values::emit_expr_js;

/// Collect all extern contracts from the file and its imports.
fn collect_extern_contracts(files: &[&roca::SourceFile]) -> Vec<(String, Vec<roca::FnSignature>)> {
    let mut result = Vec::new();
    for file in files {
        for item in &file.items {
            if let roca::Item::ExternContract(c) = item {
                result.push((c.name.clone(), c.functions.clone()));
            }
        }
    }
    result
}

/// Check whether a crash chain ends with `halt`.
fn chain_has_halt(chain: &[roca::CrashStep]) -> bool {
    chain.last().map_or(false, |s| matches!(s, roca::CrashStep::Halt))
}

/// Check if any branch of the handler ends with halt.
fn handler_has_halt(handler: &roca::CrashHandler) -> bool {
    match &handler.strategy {
        roca::CrashHandlerKind::Simple(chain) => chain_has_halt(chain),
        roca::CrashHandlerKind::Detailed { arms, default } => {
            arms.iter().any(|arm| chain_has_halt(&arm.chain))
                || default.as_ref().map_or(false, |c| chain_has_halt(c))
        }
    }
}

/// Extract the args from the first test case (any variant), or return empty vec.
fn first_test_args(test: &roca::TestBlock) -> Vec<String> {
    let args = match test.cases.first() {
        Some(roca::TestCase::Equals { args, .. }) => args,
        Some(roca::TestCase::IsOk { args }) => args,
        Some(roca::TestCase::IsErr { args, .. }) => args,
        _ => return Vec::new(),
    };
    args.iter().map(|a| emit_expr_js(a)).collect()
}

/// For a given CrashHandler, parse the call field (e.g. "Http.fetch") into
/// (contract_name, method_name). Returns None if it doesn't match the dot pattern.
fn parse_crash_call(call: &str) -> Option<(&str, &str)> {
    let dot = call.find('.')?;
    Some((&call[..dot], &call[dot + 1..]))
}

/// Generate auto error mock tests for a single function or method.
/// Returns (js_code, test_count).
fn generate_error_tests_for_fn(
    fn_label: &str,
    call_expr: &str,
    f: &roca::FnDef,
    extern_contracts: &[(String, Vec<roca::FnSignature>)],
    is_async: bool,
) -> (String, usize) {
    // Only for functions that return err and have crash with halt
    if !f.returns_err {
        return (String::new(), 0);
    }
    let crash = match &f.crash {
        Some(c) => c,
        None => return (String::new(), 0),
    };
    let test = match &f.test {
        Some(t) => t,
        None => return (String::new(), 0),
    };

    let args_js = first_test_args(test);
    let args_str = args_js.join(", ");

    let mut tests = Vec::new();
    let mut count = 0;

    for handler in &crash.handlers {
        if !handler_has_halt(handler) {
            continue;
        }

        let (contract_name, method_name) = match parse_crash_call(&handler.call) {
            Some(pair) => pair,
            None => continue,
        };

        // Find the extern contract and its method signature
        let sigs = match extern_contracts.iter().find(|(name, _)| name == contract_name) {
            Some((_, sigs)) => sigs,
            None => continue,
        };
        let sig = match sigs.iter().find(|s| s.name == method_name) {
            Some(s) => s,
            None => continue,
        };

        if !sig.returns_err || sig.errors.is_empty() {
            continue;
        }

        let mock_var = contract_name.to_string();

        for err_decl in &sig.errors {
            let err_name = &err_decl.name;
            let err_msg = &err_decl.message;
            let test_label = format!("{}[err:{}]", fn_label, err_name);

            let maybe_await = if is_async { "await " } else { "" };

            let js = format!(
                "\
// Auto error test: {contract}.{method} -> err.{err}
{{
    const _save = {mock}.{method};
    {mock}.{method} = function() {{ return {{value: null, err: {{name: \"{err}\", message: \"{msg}\"}}}}; }};
    const _actual = {await_kw}{call}({args});
    if (_actual.err && _actual.err.name === \"{err}\") {{ _passed++; }} else {{ _failed++; console.log(\"FAIL: {label}\"); }}
    {mock}.{method} = _save;
}}",
                contract = contract_name,
                method = method_name,
                err = err_name,
                msg = err_msg,
                mock = mock_var,
                await_kw = maybe_await,
                call = call_expr,
                args = args_str,
                label = test_label,
            );
            tests.push(js);
            count += 1;
        }
    }

    (tests.join("\n"), count)
}

/// Generate all auto error mock tests for the file.
/// Returns (js_code, test_count).
pub(crate) fn generate_error_mock_tests(
    file: &roca::SourceFile,
    all_files: &[&roca::SourceFile],
) -> (String, usize) {
    let extern_contracts = collect_extern_contracts(all_files);
    if extern_contracts.is_empty() {
        return (String::new(), 0);
    }

    let mut all_js = Vec::new();
    let mut total_count = 0;

    for item in &file.items {
        match item {
            roca::Item::Function(f) => {
                let is_async = super::super::functions::body_has_wait(&f.body);
                let (js, count) = generate_error_tests_for_fn(
                    &f.name,
                    &f.name,
                    f,
                    &extern_contracts,
                    is_async,
                );
                if count > 0 {
                    all_js.push(js);
                    total_count += count;
                }
            }
            roca::Item::Struct(s) => {
                for method in &s.methods {
                    let label = format!("{}.{}", s.name, method.name);
                    let call_expr = format!("{}.{}", s.name, method.name);
                    let is_async = super::super::functions::body_has_wait(&method.body);
                    let (js, count) = generate_error_tests_for_fn(
                        &label,
                        &call_expr,
                        method,
                        &extern_contracts,
                        is_async,
                    );
                    if count > 0 {
                        all_js.push(js);
                        total_count += count;
                    }
                }
            }
            _ => {}
        }
    }

    if all_js.is_empty() {
        return (String::new(), 0);
    }

    (all_js.join("\n"), total_count)
}
