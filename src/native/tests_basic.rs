//! Core language tests

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
fn add() {
    let mut m = jit("pub fn add(a: Number, b: Number) -> Number { return a + b }");
    let f = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(call_f64(&mut m, "add", 2)) };
    assert_eq!(f(37.0, 5.0), 42.0);
    assert_eq!(f(-10.0, 10.0), 0.0);
}

#[test]
fn if_else() {
    let mut m = jit(r#"
        pub fn clamp(n: Number) -> Number {
            if n > 100 { return 100 }
            if n < 0 { return 0 }
            return n
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "clamp", 1)) };
    assert_eq!(f(50.0), 50.0);
    assert_eq!(f(150.0), 100.0);
    assert_eq!(f(-10.0), 0.0);
}

#[test]
fn multiply() {
    let mut m = jit("pub fn square(n: Number) -> Number { return n * n }");
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "square", 1)) };
    assert_eq!(f(5.0), 25.0);
    assert_eq!(f(-3.0), 9.0);
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
fn while_loop() {
    let mut m = jit(r#"
        pub fn count_to(n: Number) -> Number {
            let i = 0
            while i < n { i = i + 1 }
            return i
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "count_to", 1)) };
    assert_eq!(f(5.0), 5.0);
    assert_eq!(f(100.0), 100.0);
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
fn match_expression() {
    let mut m = jit(r#"
        pub fn describe(n: Number) -> Number {
            const result = match n {
                1 => 10
                2 => 20
                _ => 0
            }
            return result
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "describe", 1)) };
    assert_eq!(f(1.0), 10.0);
    assert_eq!(f(2.0), 20.0);
    assert_eq!(f(99.0), 0.0);
}

#[test]
fn break_in_while() {
    let mut m = jit(r#"
        pub fn find_five(n: Number) -> Number {
            let i = 0
            while i < n {
                if i == 5 { break }
                i = i + 1
            }
            return i
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "find_five", 1)) };
    assert_eq!(f(10.0), 5.0);
    assert_eq!(f(3.0), 3.0);
}

#[test]
fn string_length() {
    let mut m = jit(r#"
        pub fn len(s: String) -> Number {
            return s.length
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    let id = m.declare_function("len", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> f64>(m.get_finalized_function(id)) };
    assert_eq!(f(b"hello\0".as_ptr()), 5.0);
    assert_eq!(f(b"\0".as_ptr()), 0.0);
}

#[test]
fn array_literal_and_index() {
    let mut m = jit(r#"
        pub fn second() -> Number {
            const arr = [10, 20, 30]
            return arr[1]
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "second", 0)) };
    assert_eq!(f(), 20.0);
}

#[test]
fn array_push_and_len() {
    let mut m = jit(r#"
        pub fn build() -> Number {
            const arr = [1, 2]
            arr.push(3)
            return arr.length
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "build", 0)) };
    assert_eq!(f(), 3.0);
}

#[test]
fn nested_if_else() {
    let mut m = jit(r#"
        pub fn classify(n: Number) -> Number {
            if n > 0 {
                if n > 100 {
                    return 2
                }
                return 1
            } else {
                return 0
            }
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "classify", 1)) };
    assert_eq!(f(50.0), 1.0);
    assert_eq!(f(200.0), 2.0);
    assert_eq!(f(-5.0), 0.0);
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
fn multiple_match_types() {
    let mut m = jit(r#"
        pub fn label(s: String) -> String {
            return match s {
                "a" => "alpha"
                "b" => "beta"
                _ => "unknown"
            }
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("label", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(m.get_finalized_function(id)) };
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(f(b"a\0".as_ptr()) as *const i8) }.to_str().unwrap(), "alpha");
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(f(b"b\0".as_ptr()) as *const i8) }.to_str().unwrap(), "beta");
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(f(b"x\0".as_ptr()) as *const i8) }.to_str().unwrap(), "unknown");
}

#[test]
fn continue_in_loop() {
    let mut m = jit(r#"
        pub fn sum_skip_three(n: Number) -> Number {
            let total = 0
            let i = 0
            while i < n {
                i = i + 1
                if i == 3 { continue }
                total = total + i
            }
            return total
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "sum_skip_three", 1)) };
    // 1 + 2 + 4 + 5 = 12
    assert_eq!(f(5.0), 12.0);
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
fn error_return_and_destructure() {
    let mut m = jit(r#"
        pub fn validate(n: Number) -> Number, err {
            if n < 0 { return err.negative }
            return n * 2
        }
        pub fn safe_double(n: Number) -> Number {
            let result, failed = validate(n)
            if failed { return 0 }
            return result
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "safe_double", 1)) };
    assert_eq!(f(5.0), 10.0);
    assert_eq!(f(-3.0), 0.0);
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
fn struct_create_and_access() {
    let mut m = jit(r#"
        pub fn get_x() -> Number {
            const p = Point { x: 10, y: 20 }
            return p.x + p.y
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "get_x", 0)) };
    assert_eq!(f(), 30.0);
}

#[test]
fn string_includes() {
    let mut m = jit(r#"
        pub fn has_world(s: String) -> Number {
            if s.includes("world") { return 1 }
            return 0
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    let id = m.declare_function("has_world", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> f64>(m.get_finalized_function(id)) };
    assert_eq!(f(b"hello world\0".as_ptr()), 1.0);
    assert_eq!(f(b"hello\0".as_ptr()), 0.0);
}

#[test]
fn string_trim_upper_lower() {
    let mut m = jit(r#"
        pub fn clean(s: String) -> String {
            return s.trim().toUpperCase()
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("clean", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(m.get_finalized_function(id)) };
    let result = f(b"  hello  \0".as_ptr());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "HELLO");
}

#[test]
fn string_slice() {
    let mut m = jit(r#"
        pub fn first_three(s: String) -> String {
            return s.slice(0, 3)
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("first_three", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(m.get_finalized_function(id)) };
    let result = f(b"abcdef\0".as_ptr());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "abc");
}

#[test]
fn string_index_of() {
    let mut m = jit(r#"
        pub fn find_pos(s: String) -> Number {
            return s.indexOf("world")
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    let id = m.declare_function("find_pos", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> f64>(m.get_finalized_function(id)) };
    assert_eq!(f(b"hello world\0".as_ptr()), 6.0);
    assert_eq!(f(b"hello\0".as_ptr()), -1.0);
}

#[test]
fn array_map() {
    let mut m = jit(r#"
        pub fn doubled() -> Number {
            const arr = [1, 2, 3]
            const result = arr.map(fn(x) -> x * 2)
            return result[0] + result[1] + result[2]
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "doubled", 0)) };
    assert_eq!(f(), 12.0); // 2 + 4 + 6
}

#[test]
fn array_filter() {
    let mut m = jit(r#"
        pub fn count_all() -> Number {
            const arr = [1, 2, 3]
            const result = arr.filter(fn(x) -> x > 0)
            return result.length
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "count_all", 0)) };
    assert_eq!(f(), 3.0);
}

#[test]
fn chained_string_methods() {
    let mut m = jit(r#"
        pub fn process(s: String) -> String {
            return s.trim().toLowerCase()
        }
    "#);
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
    let id = m.declare_function("process", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(*const u8) -> *const u8>(m.get_finalized_function(id)) };
    let result = f(b"  HELLO WORLD  \0".as_ptr());
    assert_eq!(unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap(), "hello world");
}

#[test]
fn crash_fallback() {
    let mut m = jit(r#"
        pub fn risky(n: Number) -> Number, err {
            if n < 0 { return err.negative }
            return n * 2
        }
        pub fn safe(n: Number) -> Number {
            return risky(n)
        crash {
            risky -> fallback(0)
        }}
    "#);
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "safe", 1)) };
    assert_eq!(f(5.0), 10.0);
    assert_eq!(f(-3.0), 0.0);
}

#[test]
fn crash_halt_propagates() {
    let mut m = jit(r#"
        pub fn inner(n: Number) -> Number, err {
            if n == 0 { return err.zero }
            return 100 / n
        }
        pub fn outer(n: Number) -> Number, err {
            return inner(n)
        crash {
            inner -> halt
        }}
    "#);
    // Call outer with error — should propagate
    let mut sig = m.make_signature();
    sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
    sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I8));
    let id = m.declare_function("outer", cranelift_module::Linkage::Export, &sig).unwrap();
    let f = unsafe { std::mem::transmute::<_, fn(f64) -> (f64, u8)>(m.get_finalized_function(id)) };
    let (val, err) = f(5.0);
    assert_eq!(val, 20.0);
    assert_eq!(err, 0);
    let (_val, err) = f(0.0);
    assert_ne!(err, 0); // Error propagated
}

#[test]
fn native_test_runner_equality() {
    let source = crate::parse::parse(r#"
        pub fn add(a: Number, b: Number) -> Number {
            return a + b
        test {
            self(1, 2) == 3
            self(0, 0) == 0
            self(-1, 1) == 0
        }}
    "#);
    let result = test_runner::run_tests(&source);
    assert_eq!(result.passed, 3, "output: {}", result.output);
    assert_eq!(result.failed, 0, "output: {}", result.output);
}

#[test]
fn native_test_runner_err() {
    let source = crate::parse::parse(r#"
        pub fn validate(n: Number) -> Number, err {
            if n < 0 { return err.negative }
            return n
        test {
            self(5) == 5
            self(-1) is err.negative
            self(0) is Ok
        }}
    "#);
    let result = test_runner::run_tests(&source);
    assert_eq!(result.passed, 3, "output: {}", result.output);
    assert_eq!(result.failed, 0, "output: {}", result.output);
}

#[test]
fn native_test_runner_failing() {
    let source = crate::parse::parse(r#"
        pub fn double(n: Number) -> Number {
            return n * 3
        test {
            self(2) == 4
        }}
    "#);
    let result = test_runner::run_tests(&source);
    assert_eq!(result.passed, 0);
    assert_eq!(result.failed, 1);
}

#[test]
fn mock_extern_fn() {
    let mut m = jit(r#"
        extern fn fetch_price() -> Number {
        mock {
            fetch_price -> 42
        }}
        pub fn get_price() -> Number {
            return fetch_price()
        }
    "#);
    let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "get_price", 0)) };
    assert_eq!(f(), 42.0);
}

#[test]
fn mock_extern_fn_with_err() {
    let source = crate::parse::parse(r#"
        extern fn load(id: Number) -> String, err {
            err not_found = "not found"
        mock {
            load -> "cached"
        }}
        pub fn safe_load(id: Number) -> String {
            return load(id)
        crash {
            load -> fallback("default")
        }
        test {
            self(1) == "cached"
        }}
    "#);
    let result = test_runner::run_tests(&source);
    assert_eq!(result.passed, 1, "output: {}", result.output);
    assert_eq!(result.failed, 0, "output: {}", result.output);
}
