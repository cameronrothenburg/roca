use std::process::Command;

/// Compile roca source to JS, append test script, run via bun, return stdout.
/// Panics with full JS + stderr if execution fails.
pub fn run(source: &str, test_script: &str) -> String {
    let file = roca::parse::parse(source);
    let js = roca::emit::emit(&file);

    // Strip "export " so we can run standalone
    let js = js.replace("export ", "");
    let full = format!("{}\n{}", js, test_script);

    let output = Command::new("bun")
        .arg("-e")
        .arg(&full)
        .output()
        .expect("failed to run bun");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        panic!(
            "bun execution failed:\n\n--- JS ---\n{}\n\n--- stderr ---\n{}",
            full, stderr
        );
    }

    stdout.trim().to_string()
}

/// Compile roca source to JS, append test script, run via bun.
/// Expects execution to fail (non-zero exit).
pub fn run_expect_fail(source: &str, test_script: &str) -> String {
    let file = roca::parse::parse(source);
    let js = roca::emit::emit(&file);
    let js = js.replace("export ", "");
    let full = format!("{}\n{}", js, test_script);

    let output = Command::new("bun")
        .arg("-e")
        .arg(&full)
        .output()
        .expect("failed to run bun");

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!output.status.success(), "expected failure but got success");
    stderr.trim().to_string()
}
