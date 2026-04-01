/// Parse, check, and run tests natively via Cranelift JIT.
pub fn run(source: &str, _test_script: &str) -> String {
    let file = roca::parse::parse(source);

    let errors = roca::check::check(&file);
    let real: Vec<_> = errors.iter()
        .filter(|e| e.code != "missing-doc" && e.code != "missing-test")
        .collect();
    if !real.is_empty() {
        panic!("checker errors:\n{}", real.iter().map(|e| format!("  {}", e)).collect::<Vec<_>>().join("\n"));
    }

    let result = roca::native::test_runner::run_tests_only(&file);
    if result.failed > 0 {
        panic!("native test failed:\n{}", result.output);
    }

    // Return the JS emit for tests that inspect output
    let js = roca::emit::emit(&file);
    js.replace("export ", "")
}

/// Same as run — native testing only.
pub fn run_with_tests(source: &str, test_script: &str) -> String {
    run(source, test_script)
}
