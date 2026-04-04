//! roca-native — compiles checked AST to machine code via Cranelift JIT.
//!
//! Cranelift is a private implementation detail. The public API is:
//! - `compile()` — AST → JIT module
//! - `run_tests()` — compile + execute proof tests
//! - `call()` — call a compiled function, returns typed Value

mod builder;
mod runtime;
mod compiler;

use roca_lang::ast::{Expr, Item, Lit, SourceFile, TestCase, Type};

/// A Roca value returned from JIT execution.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(i64),   // heap pointer
    Struct(i64),   // heap pointer
    Unit,
}

/// Opaque compiled module handle. Holds the AST for type lookups.
pub struct Module {
    compiled: compiler::CompiledModule,
    source: SourceFile,
}

/// Result of running proof tests.
pub struct TestResult {
    pub passed: usize,
    pub failed: usize,
    pub output: String,
}

/// Compile a source file to a JIT module.
pub fn compile(source: &SourceFile) -> Result<Module, String> {
    let compiled = compiler::compile(source)?;
    Ok(Module { compiled, source: source.clone() })
}

/// Call a compiled function by name. The module knows the return type.
pub fn call(module: &Module, name: &str, args: &[i64]) -> Value {
    use cranelift_module::Module as ClifModule;

    let id = module.compiled.func_ids.get(name)
        .copied()
        .expect(&format!("function not found: {name}"));
    let ptr = module.compiled.jit.get_finalized_function(id);

    // Look up return type from AST
    let ret_type = find_return_type(&module.source, name);

    unsafe {
        match &ret_type {
            Type::Float => {
                let raw = call_raw_f64(ptr, args);
                Value::Float(raw)
            }
            Type::Bool => {
                let raw = call_raw_i8(ptr, args);
                Value::Bool(raw != 0)
            }
            Type::String => {
                let raw = call_raw_i64(ptr, args);
                Value::String(raw)
            }
            Type::Named(_) | Type::Array(_) | Type::Optional(_) => {
                let raw = call_raw_i64(ptr, args);
                Value::Struct(raw)
            }
            Type::Unit => {
                call_raw_i64(ptr, args);
                Value::Unit
            }
            _ => {
                // Int and everything else
                let raw = call_raw_i64(ptr, args);
                Value::Int(raw)
            }
        }
    }
}

/// Find the return type of a function by name in the AST.
fn find_return_type(source: &SourceFile, name: &str) -> Type {
    for item in &source.items {
        match item {
            Item::Function(f) if f.name == name => return f.ret.clone(),
            Item::Struct(s) => {
                for m in &s.methods {
                    let key = format!("{}.{}", s.name, m.name);
                    if key == name { return m.ret.clone(); }
                }
            }
            _ => {}
        }
    }
    Type::Int // fallback
}

unsafe fn call_raw_i64(ptr: *const u8, args: &[i64]) -> i64 {
    match args.len() {
        0 => { let f: unsafe extern "C" fn() -> i64 = std::mem::transmute(ptr); f() }
        1 => { let f: unsafe extern "C" fn(i64) -> i64 = std::mem::transmute(ptr); f(args[0]) }
        2 => { let f: unsafe extern "C" fn(i64, i64) -> i64 = std::mem::transmute(ptr); f(args[0], args[1]) }
        3 => { let f: unsafe extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(ptr); f(args[0], args[1], args[2]) }
        _ => panic!("call: too many args (max 3)"),
    }
}

unsafe fn call_raw_f64(ptr: *const u8, args: &[i64]) -> f64 {
    match args.len() {
        0 => { let f: unsafe extern "C" fn() -> f64 = std::mem::transmute(ptr); f() }
        1 => { let f: unsafe extern "C" fn(i64) -> f64 = std::mem::transmute(ptr); f(args[0]) }
        2 => { let f: unsafe extern "C" fn(i64, i64) -> f64 = std::mem::transmute(ptr); f(args[0], args[1]) }
        _ => panic!("call_f64: too many args (max 2)"),
    }
}

unsafe fn call_raw_i8(ptr: *const u8, args: &[i64]) -> u8 {
    match args.len() {
        0 => { let f: unsafe extern "C" fn() -> u8 = std::mem::transmute(ptr); f() }
        1 => { let f: unsafe extern "C" fn(i64) -> u8 = std::mem::transmute(ptr); f(args[0]) }
        2 => { let f: unsafe extern "C" fn(i64, i64) -> u8 = std::mem::transmute(ptr); f(args[0], args[1]) }
        _ => panic!("call_i8: too many args (max 2)"),
    }
}

/// Compile and run all proof tests in a source file.
pub fn run_tests(source: &SourceFile) -> TestResult {
    let module = match compile(source) {
        Ok(m) => m,
        Err(e) => return TestResult {
            passed: 0, failed: 0,
            output: format!("compile error: {e}"),
        },
    };

    let mut passed = 0;
    let mut failed = 0;
    let mut output = String::new();

    for item in &source.items {
        let func = match item {
            Item::Function(f) => f,
            _ => continue,
        };
        let test_block = match &func.test {
            Some(t) => t,
            None => continue,
        };

        for case in &test_block.cases {
            match case {
                TestCase::Equals { args, expected } => {
                    let arg_vals: Vec<i64> = args.iter().map(|a| expr_to_i64(a)).collect();
                    let result = call(&module, &func.name, &arg_vals);
                    let expected_val = expr_to_value(expected);

                    if result == expected_val {
                        passed += 1;
                        output.push_str(&format!("PASS: {}({:?}) == {:?}\n", func.name, arg_vals, expected_val));
                    } else {
                        failed += 1;
                        output.push_str(&format!("FAIL: {}({:?}) expected {:?} got {:?}\n", func.name, arg_vals, expected_val, result));
                    }
                }
            }
        }
    }

    TestResult { passed, failed, output }
}

fn expr_to_i64(expr: &Expr) -> i64 {
    match expr {
        Expr::Lit(Lit::Int(n)) => *n,
        Expr::Lit(Lit::Bool(b)) => if *b { 1 } else { 0 },
        Expr::Lit(Lit::Float(f)) => *f as i64,
        _ => 0,
    }
}

fn expr_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Lit(Lit::Int(n)) => Value::Int(*n),
        Expr::Lit(Lit::Float(f)) => Value::Float(*f),
        Expr::Lit(Lit::Bool(b)) => Value::Bool(*b),
        _ => Value::Int(0),
    }
}

#[cfg(test)]
mod tests;
