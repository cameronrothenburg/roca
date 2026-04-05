//! roca — the Roca compiler CLI.
//!
//! Thin dispatcher. All logic lives in the library crates.

use std::fs;
use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "roca", about = "The Roca compiler", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and check a .roca file (no output emitted)
    Check {
        /// Path to a .roca file
        path: PathBuf,
    },
    /// Compile a .roca file to JavaScript
    Build {
        /// Path to a .roca file
        path: PathBuf,
    },
    /// Run proof tests natively via Cranelift JIT
    Test {
        /// Path to a .roca file
        path: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { path } => cmd_check(&path),
        Command::Build { path } => cmd_build(&path),
        Command::Test { path } => cmd_test(&path),
    }
}

fn read_source(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("error reading {}: {e}", path.display());
        process::exit(1);
    })
}

fn cmd_check(path: &PathBuf) {
    let source = read_source(path);
    let result = roca_parse::parse(&source);

    for note in &result.notes {
        println!("note[{}]: {}", note.code, note.message);
    }

    if result.errors.is_empty() {
        println!("✓ {} — no errors", path.display());
    } else {
        for err in &result.errors {
            eprintln!("error[{}]: {}", err.code, err.message);
        }
        eprintln!("\n✗ {} error(s) in {}", result.errors.len(), path.display());
        process::exit(1);
    }
}

fn cmd_build(path: &PathBuf) {
    let source = read_source(path);
    let result = roca_parse::parse(&source);

    if !result.errors.is_empty() {
        for err in &result.errors {
            eprintln!("error[{}]: {}", err.code, err.message);
        }
        eprintln!("\n✗ {} error(s) — no JS emitted", result.errors.len());
        process::exit(1);
    }

    // Run proof tests natively first
    let test_result = roca_native::run_tests(&result.ast);
    if test_result.failed > 0 {
        print!("{}", test_result.output);
        eprintln!("\n✗ {} proof test(s) failed — no JS emitted", test_result.failed);
        process::exit(1);
    }
    if test_result.passed == 0 && test_result.failed == 0 && !test_result.output.is_empty() {
        eprint!("{}", test_result.output);
        eprintln!("✗ native compile error — no JS emitted");
        process::exit(1);
    }
    if test_result.passed > 0 {
        println!("{} proof test(s) passed", test_result.passed);
    }

    // Emit JS
    let js = roca_js::emit(&result.ast);

    let out_path = path.with_extension("js");
    fs::write(&out_path, &js).unwrap_or_else(|e| {
        eprintln!("error writing {}: {e}", out_path.display());
        process::exit(1);
    });

    println!("✓ built → {}", out_path.display());
}

fn cmd_test(path: &PathBuf) {
    let source = read_source(path);
    let result = roca_parse::parse(&source);

    if !result.errors.is_empty() {
        for err in &result.errors {
            eprintln!("error[{}]: {}", err.code, err.message);
        }
        eprintln!("\n✗ {} error(s)", result.errors.len());
        process::exit(1);
    }

    let test_result = roca_native::run_tests(&result.ast);
    print!("{}", test_result.output);

    if test_result.passed == 0 && test_result.failed == 0 && !test_result.output.is_empty() {
        eprintln!("✗ native compile error");
        process::exit(1);
    }

    println!("\n{} passed, {} failed", test_result.passed, test_result.failed);

    if test_result.failed > 0 {
        process::exit(1);
    }
}
