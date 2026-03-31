/// Parse, check, compile, and run Roca source. Panics on check errors or JS failures.
pub fn run(source: &str, test_script: &str) -> String {
    let file = roca::parse::parse(source);

    // Run checker — fail on real errors (skip missing-doc/missing-test for test brevity)
    let errors = roca::check::check(&file);
    let real: Vec<_> = errors.iter()
        .filter(|e| e.code != "missing-doc" && e.code != "missing-test")
        .collect();
    if !real.is_empty() {
        panic!("checker errors:\n{}", real.iter().map(|e| format!("  {}", e)).collect::<Vec<_>>().join("\n"));
    }

    let js = roca::emit::emit(&file);
    let js = js.replace("export ", "");
    let full = format!("{}\n{}", js, test_script);

    let (stdout, success) = roca::cli::runtime::run_tests(&full);

    if !success {
        panic!(
            "JS execution failed:\n\n--- JS ---\n{}\n\n--- output ---\n{}",
            full, stdout
        );
    }

    stdout.trim().to_string()
}

/// Like run(), but also includes mock objects from the test harness.
pub fn run_with_tests(source: &str, test_script: &str) -> String {
    let file = roca::parse::parse(source);

    // Run checker
    let errors = roca::check::check(&file);
    let real: Vec<_> = errors.iter()
        .filter(|e| e.code != "missing-doc" && e.code != "missing-test")
        .collect();
    if !real.is_empty() {
        panic!("checker errors:\n{}", real.iter().map(|e| format!("  {}", e)).collect::<Vec<_>>().join("\n"));
    }

    if let Some((harness_js, _)) = roca::emit::test_harness::emit_tests(&file, "__embed__", None) {
        let full = format!("{}\n{}", harness_js, test_script);

        let (stdout, success) = roca::cli::runtime::run_tests(&full);

        if !success {
            panic!(
                "JS execution failed:\n\n--- JS ---\n{}\n\n--- output ---\n{}",
                full, stdout
            );
        }

        return stdout.trim().to_string();
    }

    run(source, test_script)
}
