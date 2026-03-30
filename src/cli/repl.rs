//! Interactive REPL — parse Roca, emit JS, execute via Bun.

use std::io::{self, Write, BufRead};

pub fn run_repl() {
    println!("Roca REPL v{}", env!("CARGO_PKG_VERSION"));
    println!("Type Roca expressions or statements. :help for commands, :q to quit.");
    println!();

    let mut defs: Vec<String> = Vec::new();
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
            break;
        }
        let line = line.trim_end().to_string();

        match line.as_str() {
            ":q" | ":quit" | ":exit" => break,
            ":help" | ":h" => { print_help(); continue; }
            ":clear" | ":c" => { defs.clear(); context.clear(); println!("cleared"); continue; }
            _ => {}
        }

        context.push_str(&line);
        context.push('\n');

        let opens = context.chars().filter(|&c| c == '{').count();
        let closes = context.chars().filter(|&c| c == '}').count();
        if opens > closes { continue; }

        let input = context.trim().to_string();
        context.clear();
        if input.is_empty() { continue; }

        if is_definition(&input) {
            // Check definition parses and type-checks
            let mut src = defs.join("\n");
            src.push('\n');
            src.push_str(&input);
            match crate::parse::try_parse(&src) {
                Ok(file) => {
                    let errors = crate::check::check(&file);
                    let real: Vec<_> = errors.iter().filter(|e| e.code != "missing-doc").collect();
                    if real.is_empty() {
                        defs.push(input);
                        println!("✓ defined");
                    } else {
                        for e in &real { println!("  {}", e); }
                    }
                }
                Err(e) => println!("  parse error: {}", e),
            }
        } else {
            eval_expr(&input, &defs);
        }
    }
    println!("bye");
}

fn eval_expr(input: &str, defs: &[String]) {
    let def_src = defs.join("\n");

    // Try as expression — wrap in a function, capture result
    let expr_src = format!(
        "{}\nfn __repl__() -> Ok {{ const __v = {} return Ok test {{}} }}",
        def_src, input
    );
    if let Ok(file) = crate::parse::try_parse(&expr_src) {
        let errors = crate::check::check(&file);
        let real: Vec<_> = errors.iter()
            .filter(|e| e.code != "missing-doc" && e.code != "missing-test")
            .collect();
        if !real.is_empty() {
            for e in &real { println!("  {}", e); }
            return;
        }
        // Emit everything, then extract the __repl__ body for the expression
        let emitted = crate::emit::emit(&file).replace("export ", "");
        // Find __repl__ function and extract its body
        if let Some(fn_start) = emitted.find("function __repl__()") {
            let rest = &emitted[fn_start..];
            if let (Some(start), Some(end)) = (rest.find('{'), rest.rfind('}')) {
                let body = rest[start+1..end].trim().replace("return null;", "");
                let def_js = emitted[..fn_start].trim();
                let run_js = format!(
                    "{}\n{}\nconst __r = __v;\nconsole.log(typeof __r === 'object' && __r !== null ? JSON.stringify(__r) : __r);",
                    def_js, body
                );
                run_bun(&run_js);
                return;
            }
        }
    }

    // Try as statement
    let stmt_src = format!(
        "{}\nfn __repl__() -> Ok {{ {} return Ok test {{}} }}",
        def_src, input
    );
    match crate::parse::try_parse(&stmt_src) {
        Ok(file) => {
            let js = crate::emit::emit(&file).replace("export ", "");
            run_bun(&format!("{}\n__repl__();", js));
        }
        Err(e) => println!("  parse error: {}", e),
    }
}

fn run_bun(js: &str) {
    super::runtime::run_js(js);
}

fn is_definition(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("pub struct ") || t.starts_with("struct ")
        || t.starts_with("pub contract ") || t.starts_with("contract ")
        || t.starts_with("pub fn ") || t.starts_with("fn ")
        || t.starts_with("extern ") || t.starts_with("enum ")
        || t.starts_with("pub enum ") || t.starts_with("import ")
        || t.contains(" satisfies ")
}

fn print_help() {
    println!("Commands:");
    println!("  :q        Exit");
    println!("  :clear    Clear definitions");
    println!("  :help     Show this help");
    println!();
    println!("Type expressions:  1 + 2");
    println!("Define functions:  fn add(a: Number, b: Number) -> Number {{ ... }}");
    println!("Call functions:    add(1, 2)");
}
