//! roca-native — compiles checked AST to machine code via Cranelift JIT.
//!
//! Cranelift is a private implementation detail — nothing outside this crate
//! touches Cranelift types. All memory operations go through roca-mem.
//!
//! # Public API
//!
//! - [`compile()`] — AST → JIT module
//! - [`call()`] — call a compiled function by name, returns typed [`Value`]
//! - [`run_tests()`] — compile + execute inline proof tests
//!
//! # Architecture
//!
//! - `compiler.rs` — AST walker emitting Cranelift IR through the builder
//! - `builder.rs` — wraps `FunctionBuilder`, no Cranelift types leak
//! - `runtime.rs` — registers roca-mem symbols into the JIT module
//!
//! The `call()` function looks up the return type from the AST and dispatches
//! to the correct calling convention (i64, f64, or i8 return).

mod builder;
mod runtime;
mod compiler;

use roca_lang::ast::{Expr, ExprKind, Item, Lit, SourceFile, TestCase, Type};

/// A Roca value — used for args, return values, and test expectations.
///
/// `String` carries either a heap pointer (from JIT) or expected content (from test).
/// `PartialEq` compares by content — reading the heap pointer via roca-mem when needed.
#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(i64),            // heap pointer (JIT return) or 0 with ExpectedString
    ExpectedString(String), // string content (test expectation)
    Struct(i64),
    Unit,
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => (a - b).abs() < 1e-10,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Unit, Value::Unit) => true,
            (Value::Struct(a), Value::Struct(b)) => a == b,
            // String comparison: read heap pointer content via roca-mem
            (Value::String(ptr), Value::ExpectedString(expected)) |
            (Value::ExpectedString(expected), Value::String(ptr)) => {
                roca_mem::read_cstr(*ptr) == expected.as_str()
            }
            (Value::String(a), Value::String(b)) => {
                roca_mem::read_cstr(*a) == roca_mem::read_cstr(*b)
            }
            (Value::ExpectedString(a), Value::ExpectedString(b)) => a == b,
            _ => false,
        }
    }
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

/// Call a compiled function by name with typed arguments.
/// Uses a compiled shim that unpacks args from a buffer — supports any number of params.
pub fn call(module: &Module, name: &str, args: &[Value]) -> Value {
    use cranelift_module::Module as ClifModule;

    let shim_name = format!("{name}__shim");
    let shim_id = module.compiled.func_ids.get(&shim_name)
        .copied()
        .unwrap_or_else(|| panic!("shim not found: {shim_name}"));
    let shim_ptr = module.compiled.jit.get_finalized_function(shim_id);
    let ret_type = find_return_type(&module.source, name);

    // Pack all args as i64 into a contiguous buffer
    let raw_args: Vec<i64> = args.iter().map(|v| match v {
        Value::Int(n) => *n,
        Value::Float(f) => i64::from_ne_bytes(f.to_ne_bytes()),
        Value::Bool(b) => if *b { 1 } else { 0 },
        Value::String(p) | Value::Struct(p) => *p,
        Value::ExpectedString(_) => panic!("ExpectedString cannot be used as a call argument"),
        Value::Unit => 0,
    }).collect();

    // Call the shim: (args_ptr: *const i64) -> i64
    let raw_result = unsafe {
        let shim: unsafe extern "C" fn(*const i64) -> i64 = std::mem::transmute(shim_ptr);
        shim(raw_args.as_ptr())
    };

    // Interpret the unified i64 result based on return type
    match &ret_type {
        Type::Float => Value::Float(f64::from_ne_bytes(raw_result.to_ne_bytes())),
        Type::Bool => Value::Bool(raw_result != 0),
        Type::String => Value::String(raw_result),
        Type::Named(_) | Type::Array(_) | Type::Optional(_) => Value::Struct(raw_result),
        Type::Unit => Value::Unit,
        _ => Value::Int(raw_result),
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
                    // Convert all args — skip test case if any arg is unsupported
                    let mut arg_vals = Vec::new();
                    let mut skip = false;
                    for a in args {
                        match expr_to_value(a) {
                            Some(v) => arg_vals.push(v),
                            None => {
                                failed += 1;
                                output.push_str(&format!("SKIP: {}(...) — unsupported arg expression\n", func.name));
                                skip = true;
                                break;
                            }
                        }
                    }
                    if skip { continue; }

                    let expected_val = match expr_to_value(expected) {
                        Some(v) => v,
                        None => {
                            failed += 1;
                            output.push_str(&format!("SKIP: {}(...) — unsupported expected expression\n", func.name));
                            continue;
                        }
                    };

                    let result = call(&module, &func.name, &arg_vals);

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

fn expr_to_value(expr: &Expr) -> Option<Value> {
    match &expr.kind {
        ExprKind::Lit(Lit::Int(n)) => Some(Value::Int(*n)),
        ExprKind::Lit(Lit::Float(f)) => Some(Value::Float(*f)),
        ExprKind::Lit(Lit::Bool(b)) => Some(Value::Bool(*b)),
        ExprKind::Lit(Lit::String(s)) => Some(Value::ExpectedString(s.clone())),
        ExprKind::Lit(Lit::Unit) => Some(Value::Unit),
        ExprKind::UnaryOp { op: roca_lang::ast::UnaryOp::Neg, expr: inner } => {
            match expr_to_value(inner)? {
                Value::Int(n) => Some(Value::Int(-n)),
                Value::Float(f) => Some(Value::Float(-f)),
                other => Some(other),
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
