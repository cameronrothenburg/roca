//! Runtime function declarations and implementations.
//! Generated Cranelift code calls these Rust functions for string ops, I/O, etc.

use cranelift_codegen::ir::types;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Module, Linkage, FuncId};
use std::ffi::CStr;

/// Runtime function IDs — used when emitting calls to runtime functions.
pub struct RuntimeFuncs {
    pub print: FuncId,
    pub string_eq: FuncId,
    pub string_concat: FuncId,
    pub string_len: FuncId,
    pub string_from_f64: FuncId,
}

/// Register runtime function symbols in the JIT builder.
pub fn register_symbols(builder: &mut JITBuilder) {
    builder.symbol("roca_print", roca_print as *const u8);
    builder.symbol("roca_string_eq", roca_string_eq as *const u8);
    builder.symbol("roca_string_concat", roca_string_concat as *const u8);
    builder.symbol("roca_string_len", roca_string_len as *const u8);
    builder.symbol("roca_string_from_f64", roca_string_from_f64 as *const u8);
}

/// Declare runtime functions in the module so Cranelift code can call them.
pub fn declare_runtime(module: &mut JITModule) -> RuntimeFuncs {
    RuntimeFuncs {
        print: declare_fn(module, "roca_print", &[types::I64], &[]),
        string_eq: declare_fn(module, "roca_string_eq", &[types::I64, types::I64], &[types::I8]),
        string_concat: declare_fn(module, "roca_string_concat", &[types::I64, types::I64], &[types::I64]),
        string_len: declare_fn(module, "roca_string_len", &[types::I64], &[types::I64]),
        string_from_f64: declare_fn(module, "roca_string_from_f64", &[types::F64], &[types::I64]),
    }
}

fn declare_fn(module: &mut JITModule, name: &str, params: &[cranelift_codegen::ir::Type], returns: &[cranelift_codegen::ir::Type]) -> FuncId {
    let mut sig = module.make_signature();
    for &p in params { sig.params.push(cranelift_codegen::ir::AbiParam::new(p)); }
    for &r in returns { sig.returns.push(cranelift_codegen::ir::AbiParam::new(r)); }
    module.declare_function(name, Linkage::Import, &sig).unwrap()
}

// ─── Runtime function implementations ──────────────────
// These are called by Cranelift-generated native code.

/// Print a string to stdout.
extern "C" fn roca_print(s: i64) {
    if s == 0 {
        println!("null");
        return;
    }
    let ptr = s as *const u8;
    let cstr = unsafe { CStr::from_ptr(ptr as *const i8) };
    if let Ok(s) = cstr.to_str() {
        println!("{}", s);
    }
}

/// Compare two strings for equality. Returns 1 if equal, 0 if not.
extern "C" fn roca_string_eq(a: i64, b: i64) -> u8 {
    if a == b { return 1; }
    if a == 0 || b == 0 { return 0; }
    let a_str = unsafe { CStr::from_ptr(a as *const i8) };
    let b_str = unsafe { CStr::from_ptr(b as *const i8) };
    if a_str == b_str { 1 } else { 0 }
}

/// Concatenate two strings. Returns a pointer to a new heap-allocated string.
extern "C" fn roca_string_concat(a: i64, b: i64) -> i64 {
    let a_str = if a == 0 { "" } else {
        unsafe { CStr::from_ptr(a as *const i8) }.to_str().unwrap_or("")
    };
    let b_str = if b == 0 { "" } else {
        unsafe { CStr::from_ptr(b as *const i8) }.to_str().unwrap_or("")
    };
    let result = format!("{}{}\0", a_str, b_str);
    let ptr = result.as_ptr() as i64;
    std::mem::forget(result); // leak — TODO: refcounting
    ptr
}

/// Get string length.
extern "C" fn roca_string_len(s: i64) -> i64 {
    if s == 0 { return 0; }
    let cstr = unsafe { CStr::from_ptr(s as *const i8) };
    cstr.to_bytes().len() as i64
}

/// Convert f64 to string. Returns a pointer to a new heap-allocated string.
extern "C" fn roca_string_from_f64(n: f64) -> i64 {
    let s = if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}\0", n as i64)
    } else {
        format!("{}\0", n)
    };
    let ptr = s.as_ptr() as i64;
    std::mem::forget(s);
    ptr
}
