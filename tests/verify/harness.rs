/// Parse, check, emit JS, and run via Node. Panics on errors.
pub fn run(source: &str, test_script: &str) -> String {
    let file = roca::parse::parse(source);

    let errors = roca::check::check(&file);
    let real: Vec<_> = errors.iter()
        .filter(|e| e.code != "missing-doc" && e.code != "missing-test"
                  && e.code != "ok-on-infallible" && e.code != "reserved-name")
        .collect();
    if !real.is_empty() {
        panic!("checker errors:\n{}", real.iter().map(|e| format!("  {}", e)).collect::<Vec<_>>().join("\n"));
    }

    let js = roca::emit::emit(&file);
    let js = js.replace("export ", "");

    // Rewrite @rocalang/runtime import to local file path
    let runtime_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("packages/runtime/index.js");
    let js = js.replace(
        "import roca from \"@rocalang/runtime\";",
        &format!("import roca from \"{}\";", runtime_path.display()),
    );

    let full = format!("{}\n{}", js, test_script);

    let output = std::process::Command::new("node")
        .arg("--input-type=module")
        .arg("-e")
        .arg(&full)
        .output();

    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        }
        Ok(o) => {
            panic!(
                "JS execution failed:\n\n--- stderr ---\n{}",
                String::from_utf8_lossy(&o.stderr)
            );
        }
        Err(e) => {
            panic!("could not run node: {}", e);
        }
    }
}

/// Same as run.
pub fn run_with_tests(source: &str, test_script: &str) -> String {
    run(source, test_script)
}
