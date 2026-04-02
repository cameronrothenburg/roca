//! Native proof test runner — JIT-compiles functions and runs inline test blocks.

use std::ffi::CStr;

use roca_ast as ast;
use roca_cranelift::JitModule;
use roca_types::RocaType;
use roca_ast::{Expr, test_block::{TestBlock, TestCase}};


/// Build the shim function name for a given base name.
pub(crate) fn shim_name(base: &str) -> String {
    format!("{}__shim", base)
}

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
    if let Err(e) = super::compile_all(&mut *module, source) {
        return NativeTestResult {
            passed: 0, failed: 1,
            output: format!("compile error: {}\n", e),
        };
    }
    if let Err(e) = module.finalize() {
        return NativeTestResult {
            passed: 0, failed: 1,
            output: format!("finalize error: {}\n", e),
        };
    }

    let mut passed = 0;
    let mut failed = 0;
    let mut output = String::new();

    for item in &source.items {
        match item {
            ast::Item::Function(f) => {
                if let Some(test) = &f.test {
                    run_fn_tests(&module, f, test, &mut passed, &mut failed, &mut output);
                }
                if with_property_tests && f.is_pub && super::property_tests::all_params_generable(f) {
                    super::property_tests::run_property_tests(&module, f, None, &mut passed, &mut failed, &mut output);
                }
            }
            ast::Item::Struct(s) => {
                for method in &s.methods {
                    if let Some(test) = &method.test {
                        let mut qualified = method.clone();
                        qualified.name = format!("{}.{}", s.name, method.name);
                        run_fn_tests(&module, &qualified, test, &mut passed, &mut failed, &mut output);
                    }
                    if with_property_tests && method.is_pub && super::property_tests::all_params_generable(method) {
                        super::property_tests::run_property_tests(&module, method, Some(&s.name), &mut passed, &mut failed, &mut output);
                    }
                }
            }
            ast::Item::Satisfies(sat) => {
                for method in &sat.methods {
                    if let Some(test) = &method.test {
                        let mut qualified = method.clone();
                        qualified.name = format!("{}.{}", sat.struct_name, method.name);
                        run_fn_tests(&module, &qualified, test, &mut passed, &mut failed, &mut output);
                    }
                    if with_property_tests && method.is_pub && super::property_tests::all_params_generable(method) {
                        super::property_tests::run_property_tests(&module, method, Some(&sat.struct_name), &mut passed, &mut failed, &mut output);
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
    module: &JitModule,
    func: &ast::FnDef,
    test: &TestBlock,
    passed: &mut usize,
    failed: &mut usize,
    output: &mut String,
) {
    let name = shim_name(&func.name);
    let ptr = match module.get_function_ptr(&name) {
        Some(p) => p,
        None => {
            output.push_str(&format!("  ✗ {} (shim not found)\n", func.name));
            *failed += test.cases.len();
            return;
        }
    };
    for case in &test.cases {
        match case {
            TestCase::Equals { args, expected } => {
                run_equals_test(ptr, func, args, expected, passed, failed, output);
            }
            TestCase::IsOk { args } => {
                run_ok_test(ptr, func, args, passed, failed, output);
            }
            TestCase::IsErr { args, err_name } => {
                run_err_test(ptr, func, args, err_name, passed, failed, output);
            }
        }
    }
}

fn run_equals_test(
    ptr: *const u8,
    func: &ast::FnDef,
    args: &[Expr],
    expected: &Expr,
    passed: &mut usize,
    failed: &mut usize,
    output: &mut String,
) {
    let ret_type = RocaType::from(&func.return_type);
    let label = format_test_label(func, args);

    let (result_bits, _err) = call_fn(ptr, func, false, args);

    match ret_type {
        RocaType::Number => {
            let result = f64::from_bits(result_bits);
            let exp = expr_to_f64(expected);
            if (result - exp).abs() < 1e-10 {
                output.push_str(&format!("  ✓ {} == {}\n", label, exp));
                *passed += 1;
            } else {
                output.push_str(&format!("  ✗ {} == {} (got {})\n", label, exp, result));
                *failed += 1;
            }
        }
        RocaType::Bool => {
            let result = result_bits != 0;
            let exp = expr_to_bool(expected);
            if result == exp {
                output.push_str(&format!("  ✓ {} == {}\n", label, exp));
                *passed += 1;
            } else {
                output.push_str(&format!("  ✗ {} == {} (got {})\n", label, exp, result));
                *failed += 1;
            }
        }
        _ => {
            let result = read_cstr_safe(result_bits as usize as *const u8);
            let exp = expr_to_string(expected);
            if result == exp {
                output.push_str(&format!("  ✓ {} == \"{}\"\n", label, exp));
                *passed += 1;
            } else {
                output.push_str(&format!("  ✗ {} == \"{}\" (got \"{}\")\n", label, exp, result));
                *failed += 1;
            }
        }
    }
}

fn run_ok_test(
    ptr: *const u8,
    func: &ast::FnDef,
    args: &[Expr],
    passed: &mut usize,
    failed: &mut usize,
    output: &mut String,
) {
    if !func.returns_err { return; }
    let label = format_test_label(func, args);

    let (_val, err) = call_fn(ptr, func, false, args);
    if err == 0 {
        output.push_str(&format!("  ✓ {} is Ok\n", label));
        *passed += 1;
    } else {
        output.push_str(&format!("  ✗ {} is Ok (got err tag {})\n", label, err));
        *failed += 1;
    }
}

fn run_err_test(
    ptr: *const u8,
    func: &ast::FnDef,
    args: &[Expr],
    _err_name: &str,
    passed: &mut usize,
    failed: &mut usize,
    output: &mut String,
) {
    if !func.returns_err { return; }
    let label = format_test_label(func, args);

    let (_val, err) = call_fn(ptr, func, false, args);
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

/// Universal shim caller — packs args into a u64 array and calls the test shim.
///
/// Returns `(result_bits, err_tag)` where:
/// - `result_bits` for Number return: `f64::to_bits()` (bitcasted back via `f64::from_bits`)
/// - `result_bits` for String/Struct return: pointer as u64 (cast to `*const u8`)
/// - `result_bits` for Bool return: 0 or 1
/// - `err_tag`: 0 = Ok, non-zero = error (only meaningful when `func.returns_err`)
///
/// If `is_method` is true, a zero self-pointer is prepended to the packed args array.
pub(super) fn call_fn(
    ptr: *const u8,
    func: &ast::FnDef,
    is_method: bool,
    args: &[Expr],
) -> (u64, u8) {
    let mut string_pool: Vec<std::ffi::CString> = Vec::new();
    let mut packed: Vec<u64> = Vec::new();

    if is_method {
        packed.push(0u64); // dummy self pointer
    }

    for (arg, param) in args.iter().zip(func.params.iter()) {
        let rtype = RocaType::from(&param.type_ref);
        match rtype {
            RocaType::Number => packed.push(expr_to_f64(arg).to_bits()),
            RocaType::Bool   => packed.push(if expr_to_bool(arg) { 1 } else { 0 }),
            _ => {
                let s = if let Expr::String(s) = arg { s.as_str() } else { "" };
                let cstr = std::ffi::CString::new(s).unwrap_or_default();
                packed.push(cstr.as_ptr() as u64);
                string_pool.push(cstr);
            }
        }
    }

    unsafe {
        if func.returns_err {
            let f = std::mem::transmute::<_, fn(*const u64) -> (i64, u8)>(ptr);
            let (result, err) = f(packed.as_ptr());
            (result as u64, err)
        } else {
            let f = std::mem::transmute::<_, fn(*const u64) -> i64>(ptr);
            (f(packed.as_ptr()) as u64, 0)
        }
    }
}

pub(super) fn expr_to_f64(expr: &Expr) -> f64 {
    match expr {
        Expr::Number(n) => *n,
        Expr::Bool(true) => 1.0,
        Expr::Bool(false) => 0.0,
        Expr::BinOp { left, op: roca_ast::BinOp::Sub, right } => {
            expr_to_f64(left) - expr_to_f64(right)
        }
        Expr::BinOp { left, op: roca_ast::BinOp::Add, right } => {
            expr_to_f64(left) + expr_to_f64(right)
        }
        Expr::BinOp { left, op: roca_ast::BinOp::Mul, right } => {
            expr_to_f64(left) * expr_to_f64(right)
        }
        Expr::BinOp { left, op: roca_ast::BinOp::Div, right } => {
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
