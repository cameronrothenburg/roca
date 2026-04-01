//! Native proof test runner — JIT-compiles functions and runs inline test blocks.

use cranelift_codegen::ir::types;
use cranelift_jit::JITModule;
use cranelift_module::{Module, Linkage};
use std::ffi::CStr;

use crate::ast::{self, Expr, test_block::{TestBlock, TestCase}};
use super::types::roca_to_cranelift;

/// Result of running native proof tests
pub struct NativeTestResult {
    pub passed: usize,
    pub failed: usize,
    pub output: String,
}

/// Run all test blocks in a source file via JIT.
pub fn run_tests(source: &ast::SourceFile) -> NativeTestResult {
    run_tests_inner(source, true)
}

/// Run test blocks only — no property tests. Used by the verify harness
/// where adversarial inputs could crash the JIT-compiled code.
#[allow(dead_code)]
pub fn run_tests_only(source: &ast::SourceFile) -> NativeTestResult {
    run_tests_inner(source, false)
}

fn run_tests_inner(source: &ast::SourceFile, with_property_tests: bool) -> NativeTestResult {
    let mut module = super::create_jit_module();
    if let Err(e) = super::compile_all(&mut module, source) {
        return NativeTestResult {
            passed: 0, failed: 1,
            output: format!("compile error: {}\n", e),
        };
    }
    module.finalize_definitions().unwrap();

    let mut passed = 0;
    let mut failed = 0;
    let mut output = String::new();

    for item in &source.items {
        match item {
            ast::Item::Function(f) => {
                if let Some(test) = &f.test {
                    run_fn_tests(&mut module, f, test, &mut passed, &mut failed, &mut output);
                }
                if with_property_tests && f.is_pub && super::property_tests::all_params_generable(f) {
                    super::property_tests::run_property_tests(&mut module, f, None, &mut passed, &mut failed, &mut output);
                }
            }
            ast::Item::Struct(s) => {
                if with_property_tests {
                    for method in &s.methods {
                        if method.is_pub && super::property_tests::all_params_generable(method) {
                            super::property_tests::run_property_tests(&mut module, method, Some(&s.name), &mut passed, &mut failed, &mut output);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    output.push_str(&format!("\n{} passed {} failed\n", passed, failed));
    NativeTestResult { passed, failed, output }
}

fn run_fn_tests(
    module: &mut JITModule,
    func: &ast::FnDef,
    test: &TestBlock,
    passed: &mut usize,
    failed: &mut usize,
    output: &mut String,
) {
    for case in &test.cases {
        match case {
            TestCase::Equals { args, expected } => {
                run_equals_test(module, func, args, expected, passed, failed, output);
            }
            TestCase::IsOk { args } => {
                run_ok_test(module, func, args, passed, failed, output);
            }
            TestCase::IsErr { args, err_name } => {
                run_err_test(module, func, args, err_name, passed, failed, output);
            }
        }
    }
}

fn run_equals_test(
    module: &mut JITModule,
    func: &ast::FnDef,
    args: &[Expr],
    expected: &Expr,
    passed: &mut usize,
    failed: &mut usize,
    output: &mut String,
) {
    let ret_type = roca_to_cranelift(&func.return_type);
    let label = format_test_label(func, args);

    match ret_type {
        t if t == types::F64 => {
            let result = if func.returns_err {
                let sig = build_sig_with_err(module, func);
                let id = module.declare_function(&func.name, Linkage::Export, &sig).unwrap();
                let ptr = module.get_finalized_function(id);
                let (val, _err) = call_with_err(ptr, func, args);
                val
            } else {
                let sig = build_sig(module, func);
                let id = module.declare_function(&func.name, Linkage::Export, &sig).unwrap();
                let ptr = module.get_finalized_function(id);
                call_f64_fn(ptr, func.params.len(), args)
            };
            let exp = expr_to_f64(expected);
            if (result - exp).abs() < 1e-10 {
                output.push_str(&format!("  ✓ {} == {}\n", label, exp));
                *passed += 1;
            } else {
                output.push_str(&format!("  ✗ {} == {} (got {})\n", label, exp, result));
                *failed += 1;
            }
        }
        t if t == types::I64 => {
            // String return
            let result = if func.returns_err {
                let sig = build_sig_with_err(module, func);
                let id = module.declare_function(&func.name, Linkage::Export, &sig).unwrap();
                let ptr = module.get_finalized_function(id);
                let (val_bits, _err) = call_with_err(ptr, func, args);
                read_cstr_safe(val_bits as i64 as *const u8)
            } else {
                let sig = build_sig(module, func);
                let id = module.declare_function(&func.name, Linkage::Export, &sig).unwrap();
                let ptr = module.get_finalized_function(id);
                let result_ptr = call_str_fn(ptr, func.params.len(), args);
                read_cstr_safe(result_ptr)
            };
            let exp = expr_to_string(expected);
            if result == exp {
                output.push_str(&format!("  ✓ {} == \"{}\"\n", label, exp));
                *passed += 1;
            } else {
                output.push_str(&format!("  ✗ {} == \"{}\" (got \"{}\")\n", label, exp, result));
                *failed += 1;
            }
        }
        t if t == types::I8 => {
            // Bool return
            let sig = build_sig(module, func);
            let id = module.declare_function(&func.name, Linkage::Export, &sig).unwrap();
            let ptr = module.get_finalized_function(id);
            let result = call_bool_fn(ptr, func.params.len(), args);
            let exp = expr_to_bool(expected);
            if result == exp {
                output.push_str(&format!("  ✓ {} == {}\n", label, exp));
                *passed += 1;
            } else {
                output.push_str(&format!("  ✗ {} == {} (got {})\n", label, exp, result));
                *failed += 1;
            }
        }
        _ => {}
    }
}

fn run_ok_test(
    module: &mut JITModule,
    func: &ast::FnDef,
    args: &[Expr],
    passed: &mut usize,
    failed: &mut usize,
    output: &mut String,
) {
    if !func.returns_err { return; }
    let label = format_test_label(func, args);
    let sig = build_sig_with_err(module, func);
    let id = module.declare_function(&func.name, Linkage::Export, &sig).unwrap();
    let ptr = module.get_finalized_function(id);

    let (_val, err) = call_with_err(ptr, func, args);
    if err == 0 {
        output.push_str(&format!("  ✓ {} is Ok\n", label));
        *passed += 1;
    } else {
        output.push_str(&format!("  ✗ {} is Ok (got err tag {})\n", label, err));
        *failed += 1;
    }
}

fn run_err_test(
    module: &mut JITModule,
    func: &ast::FnDef,
    args: &[Expr],
    _err_name: &str,
    passed: &mut usize,
    failed: &mut usize,
    output: &mut String,
) {
    if !func.returns_err { return; }
    let label = format_test_label(func, args);
    let sig = build_sig_with_err(module, func);
    let id = module.declare_function(&func.name, Linkage::Export, &sig).unwrap();
    let ptr = module.get_finalized_function(id);

    let (_val, err) = call_with_err(ptr, func, args);
    if err != 0 {
        output.push_str(&format!("  ✓ {} is err.{}\n", label, _err_name));
        *passed += 1;
    } else {
        output.push_str(&format!("  ✗ {} is err.{} (got Ok)\n", label, _err_name));
        *failed += 1;
    }
}

// ─── Helpers (pub(super) for property_tests) ─────────

pub(super) fn format_test_label(func: &ast::FnDef, args: &[Expr]) -> String {
    let args_str: Vec<String> = args.iter().map(|a| format!("{:?}", a)).collect();
    format!("{}({})", func.name, args_str.join(", "))
}

pub(super) fn build_sig(module: &JITModule, func: &ast::FnDef) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    for p in &func.params {
        sig.params.push(cranelift_codegen::ir::AbiParam::new(roca_to_cranelift(&p.type_ref)));
    }
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(roca_to_cranelift(&func.return_type)));
    sig
}

pub(super) fn build_sig_with_err(module: &JITModule, func: &ast::FnDef) -> cranelift_codegen::ir::Signature {
    let mut sig = build_sig(module, func);
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(types::I8));
    sig
}

pub(super) fn call_f64_fn(ptr: *const u8, param_count: usize, args: &[Expr]) -> f64 {
    unsafe {
        match param_count {
            0 => std::mem::transmute::<_, fn() -> f64>(ptr)(),
            1 => {
                let a = expr_to_f64(&args[0]);
                std::mem::transmute::<_, fn(f64) -> f64>(ptr)(a)
            }
            2 => {
                let a = expr_to_f64(&args[0]);
                let b = expr_to_f64(&args[1]);
                std::mem::transmute::<_, fn(f64, f64) -> f64>(ptr)(a, b)
            }
            3 => {
                let a = expr_to_f64(&args[0]);
                let b = expr_to_f64(&args[1]);
                let c = expr_to_f64(&args[2]);
                std::mem::transmute::<_, fn(f64, f64, f64) -> f64>(ptr)(a, b, c)
            }
            _ => 0.0,
        }
    }
}

pub(super) fn call_str_fn(ptr: *const u8, param_count: usize, args: &[Expr]) -> *const u8 {
    let mut pool = Vec::new();
    unsafe {
        match param_count {
            0 => std::mem::transmute::<_, fn() -> *const u8>(ptr)(),
            1 => {
                let a = expr_to_arg(&args[0], &mut pool);
                match a {
                    Arg::F64(v) => std::mem::transmute::<_, fn(f64) -> *const u8>(ptr)(v),
                    Arg::Str(p) => std::mem::transmute::<_, fn(*const u8) -> *const u8>(ptr)(p),
                }
            }
            2 => {
                let a = expr_to_arg(&args[0], &mut pool);
                let b = expr_to_arg(&args[1], &mut pool);
                match (a, b) {
                    (Arg::Str(a), Arg::Str(b)) => std::mem::transmute::<_, fn(*const u8, *const u8) -> *const u8>(ptr)(a, b),
                    (Arg::Str(a), Arg::F64(b)) => std::mem::transmute::<_, fn(*const u8, f64) -> *const u8>(ptr)(a, b),
                    (Arg::F64(a), Arg::Str(b)) => std::mem::transmute::<_, fn(f64, *const u8) -> *const u8>(ptr)(a, b),
                    (Arg::F64(a), Arg::F64(b)) => std::mem::transmute::<_, fn(f64, f64) -> *const u8>(ptr)(a, b),
                }
            }
            _ => std::ptr::null(),
        }
    }
}

pub(super) fn call_bool_fn(ptr: *const u8, param_count: usize, args: &[Expr]) -> bool {
    let mut pool = Vec::new();
    unsafe {
        match param_count {
            0 => std::mem::transmute::<_, fn() -> u8>(ptr)() != 0,
            1 => {
                let a = expr_to_arg(&args[0], &mut pool);
                match a {
                    Arg::F64(v) => std::mem::transmute::<_, fn(f64) -> u8>(ptr)(v) != 0,
                    Arg::Str(p) => std::mem::transmute::<_, fn(*const u8) -> u8>(ptr)(p) != 0,
                }
            }
            _ => false,
        }
    }
}

pub(super) fn call_with_err(ptr: *const u8, func: &ast::FnDef, args: &[Expr]) -> (f64, u8) {
    unsafe {
        match func.params.len() {
            0 => std::mem::transmute::<_, fn() -> (f64, u8)>(ptr)(),
            1 => {
                let a = expr_to_f64(&args[0]);
                std::mem::transmute::<_, fn(f64) -> (f64, u8)>(ptr)(a)
            }
            2 => {
                let a = expr_to_f64(&args[0]);
                let b = expr_to_f64(&args[1]);
                std::mem::transmute::<_, fn(f64, f64) -> (f64, u8)>(ptr)(a, b)
            }
            _ => (0.0, 0),
        }
    }
}

pub(super) enum Arg {
    F64(f64),
    Str(*const u8),
}

/// Convert an Expr to a JIT-callable argument.
/// String args are stored in `string_pool` to keep them alive for the call duration.
pub(super) fn expr_to_arg<'a>(expr: &Expr, string_pool: &'a mut Vec<std::ffi::CString>) -> Arg {
    match expr {
        Expr::Number(n) => Arg::F64(*n),
        Expr::String(s) => {
            let cstr = std::ffi::CString::new(s.as_str()).unwrap_or_default();
            let ptr = cstr.as_ptr() as *const u8;
            string_pool.push(cstr);
            Arg::Str(ptr)
        }
        _ => Arg::F64(0.0),
    }
}

pub(super) fn expr_to_f64(expr: &Expr) -> f64 {
    match expr {
        Expr::Number(n) => *n,
        Expr::Bool(true) => 1.0,
        Expr::Bool(false) => 0.0,
        // Handle parsed negative numbers: 0 - N
        Expr::BinOp { left, op: crate::ast::BinOp::Sub, right } => {
            expr_to_f64(left) - expr_to_f64(right)
        }
        Expr::BinOp { left, op: crate::ast::BinOp::Add, right } => {
            expr_to_f64(left) + expr_to_f64(right)
        }
        Expr::BinOp { left, op: crate::ast::BinOp::Mul, right } => {
            expr_to_f64(left) * expr_to_f64(right)
        }
        Expr::BinOp { left, op: crate::ast::BinOp::Div, right } => {
            expr_to_f64(left) / expr_to_f64(right)
        }
        _ => 0.0,
    }
}

fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::String(s) => s.clone(),
        Expr::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        _ => String::new(),
    }
}

fn expr_to_bool(expr: &Expr) -> bool {
    match expr {
        Expr::Bool(v) => *v,
        Expr::Number(n) => *n != 0.0,
        _ => false,
    }
}

fn read_cstr_safe(ptr: *const u8) -> String {
    if ptr.is_null() { return String::new(); }
    unsafe { CStr::from_ptr(ptr as *const i8) }
        .to_str()
        .unwrap_or("")
        .to_string()
}
