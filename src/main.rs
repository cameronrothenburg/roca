//! CLI entry point for the Roca compiler.
//! Handles `roca build`, `roca check`, `roca init`, and `roca lsp` commands.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use roca_cli::config::{resolve_src_dir, resolve_out_dir};
use roca_cli::build::{build_file, build_directory};
use roca_cli::check::{check_file, check_directory};

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct CommandInfo {
    name: &'static str,
    short_desc: &'static str,
    usage: &'static str,
    long_desc: &'static str,
}

static COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        name: "init",
        short_desc: "Create a new Roca project",
        usage: "roca init <project-name>",
        long_desc: "Create a new Roca project with roca.toml, src/, and a starter file.",
    },
    CommandInfo {
        name: "skills",
        short_desc: "Generate AI assistant skills",
        usage: "roca skills [--claude]",
        long_desc: "Generate AI assistant skills.\n  --claude  Include Claude Code integration files.",
    },
    CommandInfo {
        name: "check",
        short_desc: "Parse and check rules without emitting JS",
        usage: "roca check [path]",
        long_desc: "Parse and type-check .roca files without emitting JS.\nDefaults to src/ from roca.toml if no path given.",
    },
    CommandInfo {
        name: "build",
        short_desc: "Compile .roca files to JS with proof tests",
        usage: "roca build [path] [--emit-only]",
        long_desc: "Compile .roca files to JS with proof tests.\n  --emit-only  Skip native tests, emit JS directly.",
    },
    CommandInfo {
        name: "test",
        short_desc: "Build + run proof tests, then clean output",
        usage: "roca test [path]",
        long_desc: "Build and run proof tests, then clean output.",
    },
    CommandInfo {
        name: "run",
        short_desc: "Build + execute via Node.js",
        usage: "roca run [path]",
        long_desc: "Build and execute via Node.js.",
    },
    CommandInfo {
        name: "gen-extern",
        short_desc: "Generate extern contracts from TypeScript declarations",
        usage: "roca gen-extern <file.d.ts> [--out <path>]",
        long_desc: "Generate extern contracts from TypeScript declarations.",
    },
    CommandInfo {
        name: "repl",
        short_desc: "Interactive REPL (Node default, --native for Cranelift)",
        usage: "roca repl [--native]",
        long_desc: "Interactive REPL.\n  --native  Use Cranelift JIT instead of Node.js.",
    },
    CommandInfo {
        name: "search",
        short_desc: "Search stdlib and project for types/functions",
        usage: "roca search <query>",
        long_desc: "Search stdlib and project for types, methods, and functions.",
    },
    CommandInfo {
        name: "patterns",
        short_desc: "Show coding patterns and JS integration examples",
        usage: "roca patterns",
        long_desc: "Show coding patterns and JS integration examples.",
    },
    CommandInfo {
        name: "lsp",
        short_desc: "Start language server (stdio)",
        usage: "roca lsp",
        long_desc: "Start the language server (stdio transport).",
    },
    CommandInfo {
        name: "man",
        short_desc: "Full language manual with examples",
        usage: "roca man",
        long_desc: "Print the full language manual.",
    },
];

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
        std::process::exit(1);
    }

    // Handle --help/-h on any subcommand (any position after the command)
    if args.len() >= 3
        && !matches!(args[1].as_str(), "--help" | "-h" | "help" | "--version" | "-v" | "version")
        && args[2..].iter().any(|a| a == "--help" || a == "-h")
    {
        print_subcommand_help(&args[1]);
        return;
    }

    match args[1].as_str() {
        "init" => {
            if args.len() < 3 {
                eprintln!("usage: roca init <project-name>");
                std::process::exit(1);
            }
            roca_cli::init::init_project(&args[2]);
        }
        "repl" => {
            if args.iter().any(|a| a == "--native") {
                roca_cli::repl::run_repl_native();
            } else {
                roca_cli::repl::run_repl();
            }
        }
        "skills" => {
            let with_claude = args.iter().any(|a| a == "--claude");
            roca_cli::init::generate_skills(with_claude);
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
            let emit_only = args.iter().any(|a| a == "--emit-only");
            let path_args: Vec<String> = args.iter().filter(|a| !a.starts_with("--")).cloned().collect();
            let path = resolve_path_arg(&path_args);
            if path.is_dir() {
                if emit_only {
                    eprintln!("error: --emit-only is not supported for directories");
                    std::process::exit(1);
                }
                build_directory(&path);
            } else if emit_only {
                roca_cli::build::emit_file(&path);
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

            let code = fs::read_to_string(&js_path).unwrap_or_else(|e| {
                eprintln!("error reading {}: {}", js_path.display(), e);
                std::process::exit(1);
            });

            // Run compiled JS via node (pipe via stdin to avoid arg size limits)
            use std::io::Write;
            let mut child = std::process::Command::new("node")
                .arg("--input-type=module")
                .arg("-")
                .stdin(std::process::Stdio::piped())
                .spawn()
                .unwrap_or_else(|e| {
                    eprintln!("error: could not run node: {}", e);
                    eprintln!("install Node.js or Bun to use 'roca run'");
                    std::process::exit(1);
                });
            child.stdin.take().unwrap().write_all(code.as_bytes()).unwrap();
            let status = child.wait();
            match status {
                Ok(s) if !s.success() => std::process::exit(1),
                Err(e) => {
                    eprintln!("error: could not run node: {}", e);
                    eprintln!("install Node.js or Bun to use 'roca run'");
                    std::process::exit(1);
                }
                _ => {}
            }
        }
        "search" => {
            if args.len() < 3 {
                eprintln!("usage: roca search <query>");
                std::process::exit(1);
            }
            roca_cli::search::run_search(&args[2]);
        }
        "patterns" => {
            print!("{}", include_str!("patterns.txt"));
        }
        "lsp" => {
            tokio::runtime::Runtime::new()
                .expect("failed to create tokio runtime")
                .block_on(roca_lsp::run());
        }
        "gen-extern" => {
            if args.len() < 3 {
                eprintln!("usage: roca gen-extern <file.d.ts> [--out <path>]");
                std::process::exit(1);
            }
            let dts_path = std::path::Path::new(&args[2]);

            // Output path: --out path.roca, or derive from input filename
            let out_path = if args.len() >= 5 && args[3] == "--out" {
                std::path::PathBuf::from(&args[4])
            } else {
                let stem = dts_path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("generated")
                    .trim_end_matches(".d");
                dts_path.parent().unwrap_or(std::path::Path::new(".")).join(format!("{}.roca", stem))
            };

            match roca_cli::gen_extern::generate(dts_path) {
                Ok(content) => {
                    fs::write(&out_path, &content).unwrap_or_else(|e| {
                        eprintln!("error writing {}: {}", out_path.display(), e);
                        std::process::exit(1);
                    });
                    println!("✓ {}", out_path.display());
                }
                Err(e) => {
                    eprintln!("{}", e);
                    std::process::exit(1);
                }
            }
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

fn print_subcommand_help(cmd: &str) {
    if let Some(info) = COMMANDS.iter().find(|c| c.name == cmd) {
        println!("{}\n\n{}", info.usage, info.long_desc);
    } else {
        eprintln!("unknown command: {}", cmd);
        std::process::exit(1);
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
    for cmd in COMMANDS {
        println!("  {:20} {}", cmd.name, cmd.short_desc);
    }
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
    println!("  enum                 Named constants or algebraic data types");
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