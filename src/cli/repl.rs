//! Interactive REPL — parse Roca, emit JS, execute via Bun.

use std::io::{self, Write, BufRead};

pub fn run_repl() {
    println!("Roca REPL v{}", env!("CARGO_PKG_VERSION"));
    println!("Type Roca expressions or statements. :help for commands, :q to quit.");
    println!();

    let mut history: Vec<String> = Vec::new();
    let mut context = String::new();

    loop {
        if context.is_empty() {
            print!("roca> ");
        } else {
            print!("  ... ");
        }
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line).unwrap() == 0 {
            break; // EOF
        }
        let line = line.trim_end().to_string();

        // Commands
        match line.as_str() {
            ":q" | ":quit" | ":exit" => break,
            ":help" | ":h" => {
                print_help();
                continue;
            }
            ":clear" | ":c" => {
                history.clear();
                context.clear();
                println!("cleared");
                continue;
            }
            ":history" => {
                for (i, h) in history.iter().enumerate() {
                    println!("[{}] {}", i, h);
                }
                continue;
            }
            _ => {}
        }

        // Multi-line: accumulate until braces balance
        context.push_str(&line);
        context.push('\n');

        let opens = context.chars().filter(|&c| c == '{').count();
        let closes = context.chars().filter(|&c| c == '}').count();
        if opens > closes {
            continue; // wait for more input
        }

        let input = context.trim().to_string();
        context.clear();

        if input.is_empty() {
            continue;
        }

        history.push(input.clone());
        eval_roca(&input, &history);
    }

    println!("bye");
}

fn eval_roca(input: &str, history: &[String]) {
    // Build a complete source file from history context
    let mut source = String::new();

    // Include previous definitions (structs, contracts, functions)
    for prev in &history[..history.len().saturating_sub(1)] {
        if is_definition(prev) {
            source.push_str(prev);
            source.push('\n');
        }
    }

    // If the input is a definition, just check it
    if is_definition(input) {
        source.push_str(input);
        match crate::parse::try_parse(&source) {
            Ok(file) => {
                let errors = crate::check::check(&file);
                if errors.is_empty() {
                    println!("✓ defined");
                } else {
                    for e in &errors {
                        println!("  {}", e);
                    }
                }
            }
            Err(e) => println!("  parse error: {}", e),
        }
        return;
    }

    // Wrap expression/statement in a function, emit, and run
    let wrapped = format!(
        "{}\nfn __repl__() -> String {{ return String({}) test {{}} }}",
        source, input
    );

    match crate::parse::try_parse(&wrapped) {
        Ok(file) => {
            let errors = crate::check::check(&file);
            // Filter out missing-doc since REPL functions aren't pub
            let real_errors: Vec<_> = errors.iter()
                .filter(|e| e.code != "missing-doc" && e.code != "missing-test")
                .collect();
            if !real_errors.is_empty() {
                for e in &real_errors {
                    println!("  {}", e);
                }
                return;
            }

            let js = crate::emit::emit(&file);
            // Extract just the __repl__ call
            let run_js = format!("{}\nconsole.log(__repl__());", js);
            run_bun(&run_js);
        }
        Err(_) => {
            // Try as a statement instead
            let wrapped_stmt = format!(
                "{}\nfn __repl__() -> Ok {{ {} return Ok test {{}} }}",
                source, input
            );
            match crate::parse::try_parse(&wrapped_stmt) {
                Ok(file) => {
                    let js = crate::emit::emit(&file);
                    let run_js = format!("{}\n__repl__();", js);
                    run_bun(&run_js);
                }
                Err(e) => println!("  parse error: {}", e),
            }
        }
    }
}

fn run_bun(js: &str) {
    let output = std::process::Command::new("bun")
        .arg("-e")
        .arg(js)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stdout.is_empty() {
                print!("{}", stdout);
            }
            if !stderr.is_empty() {
                eprint!("{}", stderr);
            }
        }
        Err(e) => eprintln!("  failed to run bun: {}", e),
    }
}

fn is_definition(s: &str) -> bool {
    let trimmed = s.trim();
    trimmed.starts_with("pub struct ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("pub contract ")
        || trimmed.starts_with("contract ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("fn ")
        || trimmed.starts_with("extern ")
        || trimmed.starts_with("enum ")
        || trimmed.starts_with("pub enum ")
        || trimmed.starts_with("import ")
        || trimmed.contains(" satisfies ")
}

fn print_help() {
    println!("Commands:");
    println!("  :q, :quit     Exit the REPL");
    println!("  :clear        Clear history and definitions");
    println!("  :history      Show input history");
    println!("  :help         Show this help");
    println!();
    println!("Usage:");
    println!("  Type expressions to evaluate:    1 + 2");
    println!("  Define functions:                 fn add(a: Number, b: Number) -> Number {{ ... }}");
    println!("  Call functions:                   add(1, 2)");
    println!("  Multi-line input auto-detects unbalanced braces.");
}
