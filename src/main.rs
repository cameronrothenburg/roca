mod ast;
mod parse;
mod check;
mod emit;
mod errors;

use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("usage: roca <command> [args]");
        eprintln!("  roca check <file.roca>    — parse + check rules");
        eprintln!("  roca build <file.roca>    — parse + check + emit JS");
        std::process::exit(1);
    }

    match args[1].as_str() {
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
                eprintln!("usage: roca build <file.roca>");
                std::process::exit(1);
            }
            let source = read_file(&args[2]);
            let file = parse::parse(&source);
            let errors = check::check(&file);

            if !errors.is_empty() {
                for err in &errors {
                    eprintln!("{}", err);
                }
                eprintln!("\n✗ {} error(s) — no JS emitted", errors.len());
                std::process::exit(1);
            }

            let js = emit::emit(&file);
            let out_path = args[2].replace(".roca", ".js");

            // Write main JS
            fs::write(&out_path, &js).unwrap_or_else(|e| {
                eprintln!("error writing {}: {}", out_path, e);
                std::process::exit(1);
            });

            // Emit + run test harness
            if let Some(test_js) = emit::test_harness::emit_tests(&file) {
                let test_path = args[2].replace(".roca", ".test.js");
                fs::write(&test_path, &test_js).unwrap_or_else(|e| {
                    eprintln!("error writing {}: {}", test_path, e);
                    std::process::exit(1);
                });

                // Run tests via bun
                let output = std::process::Command::new("bun")
                    .arg(&test_path)
                    .output()
                    .expect("failed to run bun");

                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if !output.status.success() {
                    eprint!("{}", stderr);
                    print!("{}", stdout);
                    // Clean up — tests failed, remove the JS
                    let _ = fs::remove_file(&out_path);
                    let _ = fs::remove_file(&test_path);
                    eprintln!("\n✗ proof tests failed — no JS emitted");
                    std::process::exit(1);
                }

                print!("{}", stdout);
                // Clean up test file
                let _ = fs::remove_file(&test_path);
            }

            println!("✓ built → {}", out_path);
        }
        _ => {
            eprintln!("unknown command: {}", args[1]);
            std::process::exit(1);
        }
    }
}

fn read_file(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {}", path, e);
        std::process::exit(1);
    })
}
