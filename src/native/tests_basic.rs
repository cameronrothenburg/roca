//! Core language tests — primitives, operators, bindings, strings, function calls

use cranelift_jit::JITModule;
use cranelift_module::Module;
use crate::native::{create_jit_module, compile_all, compile_to_object, runtime, test_runner};

fn jit(source: &str) -> JITModule {
    let file = crate::parse::parse(source);
    let mut module = create_jit_module();
    compile_all(&mut module, &file).unwrap();
    module.finalize_definitions().unwrap();
    module
}

fn sig_f64(m: &JITModule, params: usize) -> cranelift_codegen::ir::Signature {
    let mut s = m.make_signature();
    for _ in 0..params { s.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64)); }
    s.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    s
}

unsafe fn call_f64(m: &mut JITModule, name: &str, params: usize) -> *const u8 {
    let sig = sig_f64(m, params);
    let id = m.declare_function(name, cranelift_module::Linkage::Export, &sig).unwrap();
    m.get_finalized_function(id)
}

#[test]
fn init() { drop(create_jit_module()); }

#[test]
fn return_constant() {
    let mut m = jit("pub fn answer() -> Number { return 42 }");
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "answer", 0)) };
    assert_eq!(f(), 42.0);
}

#[test]
fn add() {
    let mut m = jit("pub fn add(a: Number, b: Number) -> Number { return a + b }");
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "add", 2)) };
    assert_eq!(f(37.0, 5.0), 42.0);
    assert_eq!(f(-10.0, 10.0), 0.0);
}

#[test]
fn multiply() {
    let mut m = jit("pub fn square(n: Number) -> Number { return n * n }");
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "square", 1)) };
    assert_eq!(f(5.0), 25.0);
    assert_eq!(f(-3.0), 9.0);
}

#[test]
fn modulo_and_subtraction() {
    let mut m = jit(r#"
        pub fn sub(a: Number, b: Number) -> Number { return a - b }
        pub fn div(a: Number, b: Number) -> Number { return a / b }
    "#);
    let sub = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "sub", 2)) };
    let div = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "div", 2)) };
    assert_eq!(sub(10.0, 3.0), 7.0);
    assert_eq!(div(10.0, 2.0), 5.0);
}

#[test]
fn const_binding() {
    let mut m = jit(r#"
        pub fn double_add(a: Number, b: Number) -> Number {
            const sum = a + b
            return sum + sum
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "double_add", 2)) };
    assert_eq!(f(3.0, 4.0), 14.0);
}

#[test]
fn not_operator() {
    let mut m = jit(r#"
        pub fn negate(n: Number) -> Number {
            if !(n > 0) { return 1 }
            return 0
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "negate", 1)) };
    assert_eq!(f(-5.0), 1.0);
    assert_eq!(f(5.0), 0.0);
}

#[test]
fn and_or() {
    let mut m = jit(r#"
        pub fn both(a: Number, b: Number) -> Number {
            if a > 0 && b > 0 { return 1 }
            return 0
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "both", 2)) };
    assert_eq!(f(1.0, 1.0), 1.0);
    assert_eq!(f(1.0, -1.0), 0.0);
}

#[test]
fn function_calls() {
    let mut m = jit(r#"
        pub fn add(a: Number, b: Number) -> Number { return a + b }
        pub fn double(n: Number) -> Number { return add(n, n) }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "double", 1)) };
    assert_eq!(f(5.0), 10.0);
    assert_eq!(f(21.0), 42.0);
}

#[test]
fn string_literal() {
    let mut m = jit(r#"pub fn greeting() -> String { return "hello" }"#);
    let mut sig = m.make_signature();
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("greeting", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn() -> *const u8>(m.get_finalized_function(id)) };
    let result = f();
    assert!(!result.is_null());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "hello");
}

#[test]
fn string_equality() {
    let mut m = jit(r#"
        pub fn is_hello(s: String) -> Bool {
            if s == "hello" { return true }
            return false
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I8));
    let id = m.declare_function("is_hello", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> u8>(m.get_finalized_function(id)) };
    assert_eq!(f(b"hello\0".as_ptr()), 1);
    assert_eq!(f(b"world\0".as_ptr()), 0);
}

#[test]
fn string_concat() {
    let mut m = jit(r#"
        pub fn greet(name: String) -> String {
            return "hello " + name
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("greet", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(m.get_finalized_function(id)) };
    let result = f(b"world\0".as_ptr());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "hello world");
}

#[test]
fn string_interpolation() {
    let mut m = jit(r#"
        pub fn greet(name: String) -> String {
            return "hello {name}!"
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("greet", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(m.get_finalized_function(id)) };
    let result = f(b"world\0".as_ptr());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "hello world!");
}

#[test]
fn number_to_string() {
    let mut m = jit(r#"
        pub fn show(n: Number) -> String {
            return "{n} items"
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("show", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> *const u8>(m.get_finalized_function(id)) };
    let result = f(42.0);
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "42 items");
}

#[test]
fn method_to_string() {
    let mut m = jit(r#"
        pub fn num_to_str(n: Number) -> String {
            return n.toString()
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("num_to_str", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> *const u8>(m.get_finalized_function(id)) };
    let result = f(42.0);
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "42");
}

#[test]
fn raw_cranelift() {
    use cranelift_codegen::ir::{types, AbiParam, InstBuilder};
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
    use cranelift_module::Linkage;

    let mut module = create_jit_module();
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64));
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));

    let func_id = module.declare_function("test_add", Linkage::Export, &sig).unwrap();
    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    let mut bc = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut bc);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);
    let a = builder.block_params(entry)[0];
    let b = builder.block_params(entry)[1];
    let sum = builder.ins().iadd(a, b);
    builder.ins().return_(&[sum]);
    builder.finalize();

    module.define_function(func_id, &mut ctx).unwrap();
    module.clear_context(&mut ctx);
    module.finalize_definitions().unwrap();

    let f = unsafe { std::mem::transmute::<_, fn(i64, i64) -> i64>(module.get_finalized_function(func_id)) };
    assert_eq!(f(37, 5), 42);
}
