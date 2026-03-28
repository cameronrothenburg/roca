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
            fs::write(&out_path, &js).unwrap_or_else(|e| {
                eprintln!("error writing {}: {}", out_path, e);
                std::process::exit(1);
            });
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
