//! CLI entry point for the Roca compiler.
//! Handles `roca build`, `roca check`, `roca init`, and `roca lsp` commands.

mod ast;
mod constants;
mod parse;
mod check;
mod cli;
mod emit;
mod errors;
mod init;
mod lsp;
mod resolve;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use cli::config::{resolve_src_dir, resolve_out_dir};
use cli::build::{build_file, build_directory};
use cli::check::{check_file, check_directory};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
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
            let path = resolve_path_arg(&args);
            if path.is_dir() {
                check_directory(&path);
            } else {
                check_file(&path);
            }
        }
        "build" => {
            let path = resolve_path_arg(&args);
            if path.is_dir() {
                build_directory(&path);
            } else {
                build_file(&path);
            }
        }
        "test" => {
            let path = resolve_path_arg(&args);
            if path.is_dir() {
                build_directory(&path);
            } else {
                build_file(&path);
            }
            let out_dir = resolve_out_dir(&path);
            let _ = fs::remove_dir_all(&out_dir);
        }
        "run" => {
            let path = resolve_path_arg(&args);

            if path.is_dir() {
                build_directory(&path);
            } else {
                build_file(&path);
            }

            let out_dir = resolve_out_dir(&path);
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
        "search" => {
            if args.len() < 3 {
                eprintln!("usage: roca search <query>");
                std::process::exit(1);
            }
            cli::search::run_search(&args[2]);
        }
        "patterns" => {
            print!("{}", include_str!("patterns.txt"));
        }
        "lsp" => {
            tokio::runtime::Runtime::new()
                .expect("failed to create tokio runtime")
                .block_on(lsp::run());
        }
        "man" => {
            print_manual();
        }
        "--version" | "-v" | "version" => {
            println!("roca {}", VERSION);
        }
        "--help" | "-h" | "help" => {
            print_help();
        }
        _ => {
            eprintln!("unknown command: {}", args[1]);
            eprintln!("run 'roca help' for usage");
            std::process::exit(1);
        }
    }
}

fn resolve_path_arg(args: &[String]) -> PathBuf {
    if args.len() >= 3 {
        PathBuf::from(&args[2])
    } else {
        resolve_src_dir(Path::new("."))
    }
}

fn print_manual() {
    print!("{}", include_str!("manual.txt"));
}

fn print_help() {
    println!("roca {} — a contractual language that compiles to JS", VERSION);
    println!();
    println!("USAGE:");
    println!("  roca <command> [args]");
    println!();
    println!("COMMANDS:");
    println!("  init <name>          Create a new Roca project");
    println!("  check [path]         Parse and check rules without emitting JS");
    println!("  build [path]         Compile .roca files to JS with proof tests");
    println!("  test [path]          Build + run proof tests, then clean output");
    println!("  run [path]           Build + execute via bun");
    println!("  search <query>       Search stdlib and project for types/functions");
    println!("  patterns             Show coding patterns and JS integration examples");
    println!("  lsp                  Start language server (stdio)");
    println!("  man                  Full language manual with examples");
    println!();
    println!("OPTIONS:");
    println!("  --version, -v        Print version");
    println!("  --help, -h           Print this help");
    println!();
    println!("All commands read roca.toml for src/out paths when no [path] given.");
    println!();
    println!("LANGUAGE:");
    println!("  contract             Define a type interface (what)");
    println!("  struct               Implement a type (how)");
    println!("  satisfies            Link a struct to a contract");
    println!("  extern contract      Declare a JS runtime type shape");
    println!("  extern fn            Declare a JS runtime function");
    println!("  enum                 Define named constants");
    println!("  crash                Error handling: halt, retry, fallback, panic");
    println!("  crash chains         Compose strategies: log |> retry(3, 1000) |> halt");
    println!("  test                 Inline proof tests on every function");
    println!("  wait                 Transparent async: wait, waitAll, waitFirst");
    println!();
    println!("EXAMPLES:");
    println!("  roca init my-app && cd my-app && roca build");
    println!("  roca check src/");
    println!("  roca run src/main.roca");
}
