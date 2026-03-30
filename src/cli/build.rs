//! Build command — compiles `.roca` files (or directories) into JS, .d.ts, and test outputs.

use std::fs;
use std::path::Path;

use crate::{emit, resolve};
use super::config::*;
use super::log::{log_event, LogEvent};

pub fn build_file(path: &Path) {
    let path_str = path.display().to_string();
    let project = resolve::resolve_file(path);
    let file = match resolve_file_from_project(path, &project) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("{}\n\n✗ errors — no JS emitted", msg);
            log_event(&LogEvent::BuildFailed { file: &path_str, reason: "check errors" });
            std::process::exit(1);
        }
    };

    let js = emit::emit(&file);
    let out_dir = resolve_out_dir(path);
    fs::create_dir_all(&out_dir).unwrap_or_else(|e| {
        eprintln!("error creating {}: {}", out_dir.display(), e);
        std::process::exit(1);
    });

    write_test_runtime(&out_dir);

    let name = path.file_stem().unwrap().to_str().unwrap();
    let out_path = out_dir.join(format!("{}.js", name));

    fs::write(&out_path, &js).unwrap_or_else(|e| {
        eprintln!("error writing {}: {}", out_path.display(), e);
        std::process::exit(1);
    });

    // Write .d.ts declaration file
    let dts = emit::emit_dts(&file);
    if !dts.is_empty() {
        let dts_path = out_dir.join(format!("{}.d.ts", name));
        let _ = fs::write(&dts_path, &dts);
    }

    // Run tests
    if let Some((test_js, _count)) = emit::test_harness::emit_tests(&file, "__embed__") {
        let test_path = out_dir.join(format!("{}.test.js", name));
        fs::write(&test_path, &test_js).unwrap_or_else(|e| {
            eprintln!("error writing {}: {}", test_path.display(), e);
            std::process::exit(1);
        });

        let output = std::process::Command::new("bun")
            .arg(test_path.to_str().unwrap())
            .output()
            .expect("failed to run bun");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let (passed, failed) = parse_test_counts(&stdout);
        log_event(&LogEvent::TestResult {
            file: &path_str,
            passed, failed,
            output: &format!("{}{}", stdout, stderr),
        });

        if !output.status.success() {
            eprint!("{}", stderr);
            print!("{}", stdout);
            let _ = fs::remove_file(&out_path);
            let _ = fs::remove_file(&test_path);
            eprintln!("\n✗ proof tests failed — no JS emitted");
            log_event(&LogEvent::BuildFailed { file: &path_str, reason: "proof tests failed" });
            std::process::exit(1);
        }

        print!("{}", stdout);
        let _ = fs::remove_file(&test_path);
    }

    let _ = fs::remove_file(out_dir.join("roca-test.js"));

    // Generate package.json in out/
    let checked = vec![(path.to_path_buf(), path_str.clone(), file)];
    write_package_json(path.parent().unwrap_or(Path::new(".")), &out_dir, &checked);

    let out_str = out_path.display().to_string();
    log_event(&LogEvent::BuildSuccess { file: &path_str, output_path: &out_str });
    println!("✓ built → {}", out_path.display());

    let project_dir = path.parent().unwrap_or(std::path::Path::new("."));
    if resolve_build_mode(project_dir) == "jslib" {
        install_as_node_module(project_dir, &out_dir);
    }
}

pub fn build_directory(dir: &Path) {
    let project = resolve::resolve_directory(dir);

    let mut files = Vec::new();
    collect_roca_files(dir, &mut files);

    if files.is_empty() {
        eprintln!("no .roca files found in {}", dir.display());
        std::process::exit(1);
    }

    let out_dir = resolve_out_dir(dir);
    fs::create_dir_all(&out_dir).unwrap_or_else(|e| {
        eprintln!("error creating {}: {}", out_dir.display(), e);
        std::process::exit(1);
    });

    write_test_runtime(&out_dir);

    // ─── Phase 1: Check all files ───────────────────────
    println!("checking {} file(s)...", files.len());

    let mut checked = Vec::new();
    let mut failed_files = Vec::new();

    for file_path in &files {
        let fp_str = file_path.display().to_string();
        match resolve_file_from_project(file_path, &project) {
            Ok(f) => checked.push((file_path.clone(), fp_str, f)),
            Err(msg) => {
                eprintln!("{}", msg);
                log_event(&LogEvent::BuildFailed { file: &fp_str, reason: "check errors" });
                failed_files.push(fp_str);
            }
        }
    }

    if !failed_files.is_empty() {
        eprintln!("\n✗ {} file(s) failed checks:", failed_files.len());
        for f in &failed_files { eprintln!("  {}", f); }
        std::process::exit(1);
    }

    // ─── Phase 2: Build all JS ──────────────────────────
    println!("building {} file(s)...", checked.len());

    for (file_path, fp_str, file) in &checked {
        let js = emit::emit(file);
        let out_path = output_path_for(file_path, dir, &out_dir);

        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        fs::write(&out_path, &js).unwrap_or_else(|e| {
            eprintln!("error writing {}: {}", out_path.display(), e);
            failed_files.push(fp_str.clone());
        });

        // Write .d.ts declaration file
        let dts = emit::emit_dts(file);
        if !dts.is_empty() {
            let dts_path = out_path.with_extension("d.ts");
            let _ = fs::write(&dts_path, &dts);
        }

        let out_str = out_path.display().to_string();
        log_event(&LogEvent::BuildSuccess { file: fp_str, output_path: &out_str });
    }

    // ─── Generate out/package.json ────────────────────────
    write_package_json(dir, &out_dir, &checked);

    // ─── Phase 3: Test each file (embed mode) ────────────
    println!("testing...");

    let mut total_passed = 0;
    let mut total_failed = 0;

    for (file_path, fp_str, file) in &checked {
        let out_path = output_path_for(file_path, dir, &out_dir);
        if let Some((test_js, _count)) = emit::test_harness::emit_tests(file, "__embed__") {
            let test_path = out_path.with_extension("test.js");
            fs::write(&test_path, &test_js).unwrap_or_else(|e| {
                eprintln!("error writing {}: {}", test_path.display(), e);
            });

            let output = std::process::Command::new("bun")
                .arg(test_path.to_str().unwrap())
                .output()
                .expect("failed to run bun");

            let stdout = String::from_utf8_lossy(&output.stdout);

            let (passed, failed) = parse_test_counts(&stdout);
            log_event(&LogEvent::TestResult {
                file: fp_str,
                passed, failed,
                output: &stdout,
            });

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                print!("{}", stdout);
                eprint!("{}", stderr);
                log_event(&LogEvent::BuildFailed { file: fp_str, reason: "proof tests failed" });
                failed_files.push(fp_str.clone());
                total_failed += failed;
            } else {
                total_passed += passed;
            }

            let _ = fs::remove_file(&test_path);
        }
    }

    let _ = fs::remove_file(out_dir.join("roca-test.js"));

    println!("\n{} passed, {} failed across {} file(s)", total_passed, total_failed, files.len());

    if !failed_files.is_empty() {
        eprintln!("\n✗ {} file(s) failed:", failed_files.len());
        for f in &failed_files { eprintln!("  {}", f); }
        std::process::exit(1);
    }

    println!("✓ all files built → {}/", out_dir.display());

    if resolve_build_mode(dir) == "jslib" {
        install_as_node_module(dir, &out_dir);
    }
}

fn write_package_json(
    project_dir: &Path,
    out_dir: &Path,
    checked: &[(std::path::PathBuf, String, crate::ast::SourceFile)],
) {
    use crate::ast::Item;

    // Read name/version from roca.toml
    let mut name = "roca-package".to_string();
    let mut version = "0.1.0".to_string();
    let config_path = project_dir.join("roca.toml");
    if !config_path.exists() {
        // Walk up to find it
        let mut search = project_dir;
        loop {
            let p = search.join("roca.toml");
            if p.exists() {
                if let Ok(content) = fs::read_to_string(&p) {
                    parse_toml_field(&content, "name", &mut name);
                    parse_toml_field(&content, "version", &mut version);
                }
                break;
            }
            match search.parent() {
                Some(parent) if parent != search => search = parent,
                _ => break,
            }
        }
    } else if let Ok(content) = fs::read_to_string(&config_path) {
        parse_toml_field(&content, "name", &mut name);
        parse_toml_field(&content, "version", &mut version);
    }

    // Collect pub exports from all files
    let mut exports = Vec::new();
    for (file_path, _, file) in checked {
        let js_name = file_path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("main");

        let mut file_exports = Vec::new();
        for item in &file.items {
            match item {
                Item::Function(f) if f.is_pub => file_exports.push(f.name.clone()),
                Item::Struct(s) if s.is_pub => file_exports.push(s.name.clone()),
                Item::Enum(e) if e.is_pub => file_exports.push(e.name.clone()),
                _ => {}
            }
        }
        if !file_exports.is_empty() {
            exports.push((js_name.to_string(), file_exports));
        }
    }

    // Build exports map
    let mut export_entries = Vec::new();
    export_entries.push(format!("    \".\": \"./main.js\""));
    for (module, _) in &exports {
        if *module != "main" {
            export_entries.push(format!("    \"./{}\": \"./{}.js\"", module, module));
        }
    }

    let pkg = format!(r#"{{
  "name": "{}",
  "version": "{}",
  "type": "module",
  "main": "main.js",
  "types": "main.d.ts",
  "exports": {{
{}
  }},
  "files": [
    "*.js",
    "*.d.ts"
  ]
}}"#, name, version, export_entries.join(",\n"));

    let _ = fs::write(out_dir.join("package.json"), pkg);
}

fn parse_toml_field(content: &str, key: &str, out: &mut String) {
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with(key) && t.contains('=') {
            if let Some(val) = t.split('=').nth(1) {
                *out = val.trim().trim_matches('"').to_string();
                return;
            }
        }
    }
}

fn resolve_build_mode(project_dir: &Path) -> String {
    let mut dir = project_dir;
    loop {
        let config = dir.join("roca.toml");
        if config.exists() {
            if let Ok(content) = fs::read_to_string(&config) {
                let mut mode = String::new();
                parse_toml_field(&content, "mode", &mut mode);
                return mode;
            }
        }
        match dir.parent() {
            Some(p) if p != dir => dir = p,
            _ => return String::new(),
        }
    }
}

fn install_as_node_module(project_dir: &Path, out_dir: &Path) {
    // Read package name from out/package.json
    let pkg_path = out_dir.join("package.json");
    let pkg_name = if let Ok(content) = fs::read_to_string(&pkg_path) {
        content.lines()
            .find(|l| l.contains("\"name\""))
            .and_then(|l| l.split('"').nth(3))
            .unwrap_or("roca-lib")
            .to_string()
    } else {
        return;
    };

    let out_rel = pathdiff(out_dir, project_dir);

    println!("installing {} into node_modules...", pkg_name);

    let status = std::process::Command::new("npm")
        .args(["install", &out_rel])
        .current_dir(project_dir)
        .status();

    match status {
        Ok(s) if s.success() => {
            println!("✓ installed — import {{ }} from \"{}\"", pkg_name);
        }
        _ => {
            eprintln!("warning: npm install failed — run manually: npm install {}", out_rel);
        }
    }
}

fn pathdiff(target: &Path, base: &Path) -> String {
    let target_abs = fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let base_abs = fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    // Try to make relative
    if let Ok(rel) = target_abs.strip_prefix(&base_abs) {
        return format!("./{}", rel.display());
    }
    target_abs.display().to_string()
}

fn parse_test_counts(output: &str) -> (usize, usize) {
    for line in output.lines() {
        if line.contains("passed") && line.contains("failed") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            let passed = parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
            let failed = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            return (passed, failed);
        }
    }
    (0, 0)
}
