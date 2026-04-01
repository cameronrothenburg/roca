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
    match std::process::Command::new("node")
        .arg("--input-type=module")
        .arg("-e")
        .arg(js)
        .status()
    {
        Ok(s) if !s.success() => eprintln!("  JS execution failed (exit {})", s.code().unwrap_or(-1)),
        Err(e) => eprintln!("  error: could not run node: {}\n  install Node.js to use the JS REPL", e),
        _ => {}
    }
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

/// Native REPL — compiles and runs via Cranelift JIT.
/// Every expression is compiled to native code and executed directly.
pub fn run_repl_native() {
    println!("Roca REPL v{} (native/cranelift)", env!("CARGO_PKG_VERSION"));
    println!("Type Roca expressions. :q to quit, :clear to reset.");
    println!();

    let mut defs: Vec<String> = Vec::new();
    let mut context = String::new();

    loop {
        if context.is_empty() {
            print!("roca/native> ");
        } else {
            print!("       ... > ");
        }
        io::stdout().flush().unwrap();

        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line).unwrap() == 0 { break; }
        let line = line.trim_end().to_string();

        match line.as_str() {
            ":q" | ":quit" | ":exit" => break,
            ":help" | ":h" => { print_help(); continue; }
            ":clear" | ":c" => { defs.clear(); context.clear(); println!("cleared"); continue; }
            ":mem" => {
                let (a, f, r, rel, live) = crate::native::runtime::MEM.stats();
                println!("allocs={} frees={} retains={} releases={} live_bytes={}", a, f, r, rel, live);
                continue;
            }
            ":mem reset" => {
                crate::native::runtime::MEM.reset();
                println!("memory counters reset");
                continue;
            }
            ":debug on" => { crate::native::runtime::MEM.set_debug(true); println!("memory debug on"); continue; }
            ":debug off" => { crate::native::runtime::MEM.set_debug(false); println!("memory debug off"); continue; }
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
            eval_expr_native(&input, &defs);
        }
    }
    println!("bye");
}

fn eval_expr_native(input: &str, defs: &[String]) {
    use cranelift_module::Module;

    let def_src = defs.join("\n");

    // Try as Number first, then String
    for (ret_type, ret_cranelift, is_string) in &[
        ("Number", cranelift_codegen::ir::types::F64, false),
        ("String", cranelift_codegen::ir::types::I64, true),
    ] {
        let expr_src = format!(
            "{}\npub fn __repl__() -> {} {{ return {} test {{ }} }}",
            def_src, ret_type, input
        );

        let file = match crate::parse::try_parse(&expr_src) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let errors = crate::check::check(&file);
        let real: Vec<_> = errors.iter()
            .filter(|e| e.code != "missing-doc" && e.code != "missing-test" && e.code != "must-be-const")
            .collect();
        if !real.is_empty() { continue; }

        let mut module = crate::native::create_jit_module();
        if crate::native::compile_all(&mut module, &file).is_err() { continue; }
        if module.finalize_definitions().is_err() {
            println!("  jit error (try running with execmem permissions)");
            return;
        }

        let mut sig = module.make_signature();
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(*ret_cranelift));
        let id = match module.declare_function("__repl__", cranelift_module::Linkage::Export, &sig) {
            Ok(id) => id,
            Err(_) => continue,
        };
        let ptr = module.get_finalized_function(id);

        if *is_string {
            let f: fn() -> *const u8 = unsafe { std::mem::transmute(ptr) };
            let result = f();
            if result.is_null() {
                println!("null");
            } else {
                let s = unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap_or("?");
                println!("{}", s);
            }
        } else {
            let f: fn() -> f64 = unsafe { std::mem::transmute(ptr) };
            let result = f();
            if result.fract() == 0.0 && result.abs() < 1e15 {
                println!("{}", result as i64);
            } else {
                println!("{}", result);
            }
        }
        return;
    }

    println!("  could not evaluate expression");
}

fn print_help() {
    println!("Commands:");
    println!("  :q        Exit");
    println!("  :clear    Clear definitions");
    println!("  :help     Show this help");
    println!("  :mem      Show memory counters (native only)");
    println!("  :mem reset  Reset memory counters (native only)");
    println!("  :debug on/off  Toggle memory tracing (native only)");
    println!();
    println!("Type expressions:  1 + 2");
    println!("Define functions:  fn add(a: Number, b: Number) -> Number {{ ... }}");
    println!("Call functions:    add(1, 2)");
    println!();
    println!("Modes:");
    println!("  roca repl           V8 engine (default)");
    println!("  roca repl --native  Cranelift JIT engine");
}
