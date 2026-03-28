mod ast;
mod constants;
mod parse;
mod check;
mod emit;
mod errors;
mod init;
mod lsp;
mod resolve;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("usage: roca <command> [args]");
        eprintln!("  roca init <name>              — create a new project");
        eprintln!("  roca check <file.roca>       — parse + check rules");
        eprintln!("  roca build <file_or_dir>     — parse + check + test + emit JS");
        eprintln!("  roca run <file.roca>          — build + execute via bun");
        eprintln!("  roca lsp                     — start language server (stdio)");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "init" => {
            if args.len() < 3 {
                eprintln!("usage: roca init <project-name>");
                std::process::exit(1);
            }
            init::init_project(&args[2]);
        }
        "check" => {
            if args.len() < 3 {
                eprintln!("usage: roca check <file.roca>");
                std::process::exit(1);
            }
            let source = read_file(&args[2]);
            let file = parse::parse(&source);
            let errors = check::check(&file);

            if errors.is_empty() {
                println!("✓ all checks passed");
            } else {
                for err in &errors {
                    eprintln!("{}", err);
                }
                eprintln!("\n✗ {} error(s)", errors.len());
                std::process::exit(1);
            }
        }
        "build" => {
            if args.len() < 3 {
                eprintln!("usage: roca build <file_or_dir>");
                std::process::exit(1);
            }

            let path = Path::new(&args[2]);
            if path.is_dir() {
                build_directory(path);
            } else {
                build_file(path);
            }
        }
        "run" => {
            if args.len() < 3 {
                eprintln!("usage: roca run <file.roca>");
                std::process::exit(1);
            }
            let path = Path::new(&args[2]);

            if path.is_dir() {
                build_directory(path);
            } else {
                build_file(path);
            }

            let out_dir = resolve_out_dir(path);
            let js_path = if path.is_dir() {
                let main = out_dir.join("main.js");
                let index = out_dir.join("index.js");
                if main.exists() {
                    main
                } else if index.exists() {
                    index
                } else {
                    eprintln!("no main.js or index.js found in {}", out_dir.display());
                    std::process::exit(1);
                }
            } else {
                let name = path.file_stem().unwrap().to_str().unwrap();
                out_dir.join(format!("{}.js", name))
            };

            let status = std::process::Command::new("bun")
                .arg(js_path.to_str().unwrap())
                .status()
                .expect("failed to run bun");

            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        "lsp" => {
            tokio::runtime::Runtime::new()
                .expect("failed to create tokio runtime")
                .block_on(lsp::run());
        }
        _ => {
            eprintln!("unknown command: {}", args[1]);
            std::process::exit(1);
        }
    }
}

/// Determine the output directory — `out/` next to the source
fn resolve_out_dir(path: &Path) -> PathBuf {
    if path.is_dir() {
        path.join("out")
    } else {
        path.parent().unwrap_or(Path::new(".")).join("out")
    }
}

/// Get the output path for a source file
fn output_path_for(source_path: &Path, src_dir: &Path, out_dir: &Path) -> PathBuf {
    let relative = source_path.strip_prefix(src_dir).unwrap_or(source_path);
    out_dir.join(relative).with_extension("js")
}

fn build_file(path: &Path) {
    let project = resolve::resolve_file(path);
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let file = match project.files.get(&canonical) {
        Some(f) => f.clone(),
        None => {
            let source = read_file(path.to_str().unwrap());
            parse::parse(&source)
        }
    };
    let errors = check::check_with_registry(&file, &project.registry);

    if !errors.is_empty() {
        for err in &errors {
            eprintln!("{}", err);
        }
        eprintln!("\n✗ {} error(s) — no JS emitted", errors.len());
        std::process::exit(1);
    }

    let js = emit::emit(&file);
    let out_dir = resolve_out_dir(path);
    fs::create_dir_all(&out_dir).unwrap_or_else(|e| {
        eprintln!("error creating {}: {}", out_dir.display(), e);
        std::process::exit(1);
    });

    let name = path.file_stem().unwrap().to_str().unwrap();
    let out_path = out_dir.join(format!("{}.js", name));

    fs::write(&out_path, &js).unwrap_or_else(|e| {
        eprintln!("error writing {}: {}", out_path.display(), e);
        std::process::exit(1);
    });

    // Run proof tests
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

        if !output.status.success() {
            eprint!("{}", stderr);
            print!("{}", stdout);
            let _ = fs::remove_file(&out_path);
            let _ = fs::remove_file(&test_path);
            eprintln!("\n✗ proof tests failed — no JS emitted");
            std::process::exit(1);
        }

        print!("{}", stdout);
        let _ = fs::remove_file(&test_path);
    }

    println!("✓ built → {}", out_path.display());
}

/// Build all .roca files in a directory with shared import resolution
fn build_directory(dir: &Path) {
    let project = resolve::resolve_directory(dir);

    let mut files: Vec<_> = Vec::new();
    collect_roca_files(dir, &mut files);

    if files.is_empty() {
        eprintln!("no .roca files found in {}", dir.display());
        std::process::exit(1);
    }

    let out_dir = dir.join("out");
    fs::create_dir_all(&out_dir).unwrap_or_else(|e| {
        eprintln!("error creating {}: {}", out_dir.display(), e);
        std::process::exit(1);
    });

    println!("building {} file(s)...", files.len());

    let mut total_tests = 0;
    let mut total_passed = 0;
    let mut failed_files = Vec::new();

    for file_path in &files {
        let canonical = file_path.canonicalize().unwrap_or_else(|_| file_path.clone());
        let file = match project.files.get(&canonical) {
            Some(f) => f.clone(),
            None => {
                let source = read_file(file_path.to_str().unwrap());
                parse::parse(&source)
            }
        };
        let errors = check::check_with_registry(&file, &project.registry);

        if !errors.is_empty() {
            for err in &errors {
                eprintln!("{}", err);
            }
            failed_files.push(file_path.display().to_string());
            continue;
        }

        let js = emit::emit(&file);
        let out_path = output_path_for(file_path, dir, &out_dir);

        // Ensure parent dir exists for nested sources
        if let Some(parent) = out_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        fs::write(&out_path, &js).unwrap_or_else(|e| {
            eprintln!("error writing {}: {}", out_path.display(), e);
            failed_files.push(file_path.display().to_string());
        });

        // Run proof tests
        if let Some((test_js, count)) = emit::test_harness::emit_tests(&file, "__embed__") {
            total_tests += count;
            let test_path = out_path.with_extension("test.js");
            fs::write(&test_path, &test_js).unwrap_or_else(|e| {
                eprintln!("error writing {}: {}", test_path.display(), e);
            });

            let output = std::process::Command::new("bun")
                .arg(test_path.to_str().unwrap())
                .output()
                .expect("failed to run bun");

            let stdout = String::from_utf8_lossy(&output.stdout);

            if !output.status.success() {
                print!("{}", stdout);
                let _ = fs::remove_file(&out_path);
                failed_files.push(file_path.display().to_string());
            } else {
                if let Some(line) = stdout.lines().find(|l| l.contains("passed")) {
                    if let Some(n) = line.split_whitespace().next().and_then(|s| s.parse::<usize>().ok()) {
                        total_passed += n;
                    }
                }
            }

            let _ = fs::remove_file(&test_path);
        }
    }

    println!("\n{} tests passed across {} file(s)", total_passed, files.len());

    if !failed_files.is_empty() {
        eprintln!("\n✗ {} file(s) failed:", failed_files.len());
        for f in &failed_files {
            eprintln!("  {}", f);
        }
        std::process::exit(1);
    }

    println!("✓ all files built → {}/", out_dir.display());
}

fn collect_roca_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip the out directory
                if path.file_name().map_or(false, |n| n == "out") {
                    continue;
                }
                collect_roca_files(&path, files);
            } else if path.extension().map_or(false, |e| e == "roca") {
                files.push(path);
            }
        }
    }
}

fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {}", path, e);
        std::process::exit(1);
    })
}
