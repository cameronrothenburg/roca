//! Native engine — Cranelift JIT compilation for proof tests and native execution.
//! Experimental: use `--engine=native` to enable.

pub mod types;
pub mod helpers;
pub mod runtime;
pub mod emit;
pub mod test_runner;

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;
use cranelift_object::{ObjectBuilder, ObjectModule};

/// Create a JIT module with the Roca runtime functions registered.
pub fn create_jit_module() -> JITModule {
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
        .expect("failed to create JIT builder");
    runtime::register_symbols(&mut builder);
    JITModule::new(builder)
}

/// Compile all functions in a source file into a module.
/// Extern fns with mock blocks are compiled as stubs that return the mock value.
pub fn compile_all<M: Module>(
    module: &mut M,
    source: &crate::ast::SourceFile,
) -> Result<(), String> {
    let rt = runtime::declare_runtime(module);
    let mut compiled = emit::CompiledFuncs::new();

    // Build function return kind map from all definitions
    let func_return_kinds = emit::build_return_kind_map(source);

    // Compile mock stubs for extern fns before user functions
    for item in &source.items {
        if let crate::ast::Item::ExternFn(ef) = item {
            if let Some(mock) = &ef.mock {
                emit::compile_mock_stub(module, ef, mock, &rt, &mut compiled)?;
            }
        }
    }

    // Pre-compile closures as top-level functions
    emit::compile_closures(module, source, &rt, &mut compiled, &func_return_kinds)?;

    for item in &source.items {
        if let crate::ast::Item::Function(f) = item {
            emit::compile_function(module, f, &rt, &mut compiled, &func_return_kinds)?;
        }
    }
    Ok(())
}

/// Compile Roca source to an object file via Cranelift AOT (production).
pub fn compile_to_object(source: &crate::ast::SourceFile) -> Result<Vec<u8>, String> {
    let isa = cranelift_native::builder()
        .map_err(|e| format!("native ISA: {}", e))?
        .finish(cranelift_codegen::settings::Flags::new(cranelift_codegen::settings::builder()))
        .map_err(|e| format!("ISA build: {}", e))?;

    let builder = ObjectBuilder::new(
        isa,
        "roca_module",
        cranelift_module::default_libcall_names(),
    ).map_err(|e| format!("object builder: {}", e))?;

    let mut module = ObjectModule::new(builder);
    compile_all(&mut module, source)?;

    let product = module.finish();
    let bytes = product.emit()
        .map_err(|e| format!("emit object: {}", e))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Helpers ───────────────────────────────────────

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

    // ─── Tests ─────────────────────────────────────────

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

    #[test]
    #[test]
    fn closure_as_value() {
        let mut m = jit(r#"
            pub fn apply() -> Number {
                const double = fn(x) -> x * 2
                return double(5)
            }
        "#);
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "apply", 0)) };
        assert_eq!(f(), 10.0);
    }

    #[test]
    fn closure_arithmetic() {
        let mut m = jit(r#"
            pub fn compute() -> Number {
                const add_ten = fn(x) -> x + 10
                const sub_one = fn(x) -> x - 1
                return add_ten(sub_one(5))
            }
        "#);
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "compute", 0)) };
        assert_eq!(f(), 14.0); // (5-1)+10
    }

    #[test]
    fn closure_passed_to_function() {
        let mut m = jit(r#"
            pub fn apply_fn(n: Number, transform: fn(Number) -> Number) -> Number {
                return transform(n)
            }
            pub fn use_it() -> Number {
                const triple = fn(x) -> x * 3
                return apply_fn(4, triple)
            }
        "#);
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "use_it", 0)) };
        assert_eq!(f(), 12.0);
    }

    fn aot_produces_object() {
        let file = crate::parse::parse("pub fn add(a: Number, b: Number) -> Number { return a + b }");
        let bytes = compile_to_object(&file).unwrap();
        assert!(bytes.len() > 100, "object file too small: {} bytes", bytes.len());
        assert_eq!(&bytes[1..4], b"ELF", "expected ELF object file");
    }

    // ─── Memory Tests ──────────────────────────────────
    // All memory tests acquire MEM_TEST_LOCK to prevent parallel interference.
    // Pattern: lock → reset → compile → run → assert exact counts.

    macro_rules! mem_test {
        ($name:ident, $body:block) => {
            #[test]
            fn $name() {
                let _lock = runtime::MEM_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
                runtime::MEM.reset();
                $body
            }
        };
    }

    mem_test!(rc_alloc_and_release, {
        let ptr = runtime::roca_rc_alloc(32);
        assert_ne!(ptr, 0);
        assert_eq!(runtime::MEM.allocs.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(runtime::MEM.frees.load(std::sync::atomic::Ordering::SeqCst), 0);

        runtime::roca_rc_release(ptr);
        assert_eq!(runtime::MEM.frees.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(runtime::MEM.live_bytes.load(std::sync::atomic::Ordering::SeqCst), 0);
    });

    mem_test!(rc_retain_delays_free, {
        let ptr = runtime::roca_rc_alloc(16);
        runtime::roca_rc_retain(ptr); // refcount 2

        runtime::roca_rc_release(ptr); // refcount 1
        assert_eq!(runtime::MEM.frees.load(std::sync::atomic::Ordering::SeqCst), 0);

        runtime::roca_rc_release(ptr); // refcount 0, freed
        assert_eq!(runtime::MEM.frees.load(std::sync::atomic::Ordering::SeqCst), 1);
    });

    mem_test!(rc_null_is_safe, {
        runtime::roca_rc_retain(0);
        runtime::roca_rc_release(0);
        runtime::MEM.assert_clean();
    });

    mem_test!(rc_multiple_allocs_all_freed, {
        let ptrs: Vec<i64> = (0..10).map(|_| runtime::roca_rc_alloc(24)).collect();
        assert_eq!(runtime::MEM.allocs.load(std::sync::atomic::Ordering::SeqCst), 10);
        for ptr in ptrs { runtime::roca_rc_release(ptr); }
        runtime::MEM.assert_clean();
    });

    mem_test!(rc_shared_const_pattern, {
        let ptr = runtime::roca_rc_alloc(8);
        runtime::roca_rc_retain(ptr); // refcount 2
        runtime::roca_rc_release(ptr); // refcount 1
        runtime::roca_rc_release(ptr); // refcount 0, freed
        runtime::MEM.assert_clean();
    });

    mem_test!(mem_scope_frees_string_locals, {
        let mut m = jit(r#"
            pub fn work() -> Number {
                const s = "hello"
                const t = "world"
                return 42
            }
        "#);
        runtime::MEM.reset(); // reset after compilation
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "work", 0)) };
        assert_eq!(f(), 42.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert!(allocs >= 2, "should allocate >= 2 strings, got {}", allocs);
        assert_eq!(allocs, frees, "all string locals freed: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_return_value_not_freed, {
        let mut m = jit(r#"
            pub fn greeting() -> String {
                const extra = "unused"
                return "hello"
            }
        "#);
        let mut sig = m.make_signature();
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
        let id = m.declare_function("greeting", cranelift_module::Linkage::Export, &sig).unwrap();
        let f = unsafe { std::mem::transmute::<_, fn() -> *const u8>(m.get_finalized_function(id)) };
        runtime::MEM.reset();
        let result = f();
        assert!(!result.is_null());
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert_eq!(frees, allocs - 1, "return value NOT freed: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_struct_freed_at_scope_exit, {
        let mut m = jit(r#"
            pub fn make_point() -> Number {
                const p = Point { x: 10, y: 20 }
                return p.x
            }
        "#);
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make_point", 0)) };
        runtime::MEM.reset();
        assert_eq!(f(), 10.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert!(allocs >= 1, "should allocate struct");
        assert_eq!(allocs, frees, "struct freed: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_loop_no_leak, {
        let mut m = jit(r#"
            pub fn loop_count() -> Number {
                let i = 0
                while i < 5 {
                    const s = "temp"
                    i = i + 1
                }
                return i
            }
        "#);
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "loop_count", 0)) };
        runtime::MEM.reset();
        assert_eq!(f(), 5.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert!(allocs >= 5, "should allocate >= 5 strings, got {}", allocs);
        assert_eq!(allocs, frees, "loop locals freed: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_let_reassign_frees_old, {
        let mut m = jit(r#"
            pub fn reassign() -> Number {
                let s = "first"
                s = "second"
                s = "third"
                return 42
            }
        "#);
        runtime::MEM.reset();
        assert_eq!(unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "reassign", 0)) }(), 42.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert_eq!(allocs, 3, "should allocate 3 strings");
        assert_eq!(allocs, frees, "all reassigned freed: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_break_cleans_up, {
        let mut m = jit(r#"
            pub fn break_test() -> Number {
                let i = 0
                while i < 100 {
                    const msg = "iteration"
                    if i == 5 { break }
                    i = i + 1
                }
                return i
            }
        "#);
        runtime::MEM.reset();
        assert_eq!(unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "break_test", 0)) }(), 5.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert_eq!(allocs, frees, "break cleans up: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_array_freed_at_scope_exit, {
        let mut m = jit(r#"
            pub fn make_arr() -> Number {
                const arr = [1, 2, 3]
                return arr.length
            }
        "#);
        runtime::MEM.reset();
        assert_eq!(unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "make_arr", 0)) }(), 3.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert!(allocs >= 1, "should allocate array");
        assert_eq!(allocs, frees, "array freed: {} allocs, {} frees", allocs, frees);
    });

    // ─── Cross-function & scope tracking ──────────────

    mem_test!(mem_cross_function_ownership, {
        // B creates a string, returns it. A calls B, uses result, frees at scope exit.
        let mut m = jit(r#"
            pub fn make() -> String {
                const temp = "discarded"
                return "created"
            }
            pub fn use_it() -> Number {
                const s = make()
                return s.length
            }
        "#);
        runtime::MEM.reset();
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "use_it", 0)) };
        assert_eq!(f(), 7.0); // "created".length
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        // make() allocates "discarded" (freed inside make) + "created" (returned, freed in use_it)
        assert_eq!(allocs, frees, "cross-function: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_nested_if_scopes, {
        // Strings created in branches must all be freed
        let mut m = jit(r#"
            pub fn branchy(n: Number) -> Number {
                const a = "always"
                if n > 0 {
                    const b = "positive"
                    return 1
                } else {
                    const c = "negative"
                    return 0
                }
            }
        "#);
        runtime::MEM.reset();
        let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "branchy", 1)) };
        assert_eq!(f(5.0), 1.0);
        let (a1, f1, _, _, _) = runtime::MEM.stats();
        assert_eq!(a1, f1, "positive branch: {} allocs, {} frees", a1, f1);

        runtime::MEM.reset();
        assert_eq!(f(-5.0), 0.0);
        let (a2, f2, _, _, _) = runtime::MEM.stats();
        assert_eq!(a2, f2, "negative branch: {} allocs, {} frees", a2, f2);
    });

    mem_test!(mem_function_chain, {
        // C → B → A chain, callees defined first (native requires definition order)
        let mut m = jit(r#"
            pub fn step_c() -> String {
                const local_c = "c_local"
                return "final"
            }
            pub fn step_b() -> String {
                const local_b = "b_local"
                return step_c()
            }
            pub fn step_a() -> String {
                const local_a = "a_local"
                return step_b()
            }
        "#);
        let mut sig = m.make_signature();
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
        let id = m.declare_function("step_a", cranelift_module::Linkage::Export, &sig).unwrap();
        let f = unsafe { std::mem::transmute::<_, fn() -> *const u8>(m.get_finalized_function(id)) };
        runtime::MEM.reset();
        let result = f();
        assert!(!result.is_null());
        let s = unsafe { std::ffi::CStr::from_ptr(result as *const i8) }.to_str().unwrap();
        assert_eq!(s, "final");
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        // 4 allocs: a_local, b_local, c_local, "final"
        // 3 frees: a_local, b_local, c_local (each freed at their function's scope exit)
        // "final" returned to caller (not freed)
        assert_eq!(frees, allocs - 1, "chain: {} allocs, {} frees (1 returned)", allocs, frees);
    });

    mem_test!(mem_string_concat_intermediates, {
        // String concat creates intermediates that must be freed
        let mut m = jit(r#"
            pub fn concat_test() -> Number {
                const a = "hello"
                const b = " "
                const c = "world"
                const result = a + b + c
                return result.length
            }
        "#);
        runtime::MEM.reset();
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "concat_test", 0)) };
        assert_eq!(f(), 11.0); // "hello world"
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert_eq!(allocs, frees, "concat intermediates freed: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_multiple_returns_all_clean, {
        // Function with early returns — all paths must clean up
        let mut m = jit(r#"
            pub fn early(n: Number) -> Number {
                const always = "setup"
                if n == 1 {
                    const branch1 = "one"
                    return 1
                }
                if n == 2 {
                    const branch2 = "two"
                    return 2
                }
                const fallthrough = "default"
                return 0
            }
        "#);
        runtime::MEM.reset();
        let f = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(call_f64(&mut m, "early", 1)) };

        // Path 1: n=1
        runtime::MEM.reset();
        assert_eq!(f(1.0), 1.0);
        let (a1, f1, _, _, _) = runtime::MEM.stats();
        assert_eq!(a1, f1, "n=1 path: {} allocs, {} frees", a1, f1);

        // Path 2: n=2
        runtime::MEM.reset();
        assert_eq!(f(2.0), 2.0);
        let (a2, f2, _, _, _) = runtime::MEM.stats();
        assert_eq!(a2, f2, "n=2 path: {} allocs, {} frees", a2, f2);

        // Path 3: fallthrough
        runtime::MEM.reset();
        assert_eq!(f(99.0), 0.0);
        let (a3, f3, _, _, _) = runtime::MEM.stats();
        assert_eq!(a3, f3, "default path: {} allocs, {} frees", a3, f3);
    });

    mem_test!(mem_loop_with_string_reassign, {
        // String reassignment inside a loop — old values freed each iteration
        let mut m = jit(r#"
            pub fn build() -> Number {
                let msg = "start"
                let i = 0
                while i < 3 {
                    msg = "iter"
                    i = i + 1
                }
                return i
            }
        "#);
        runtime::MEM.reset();
        assert_eq!(unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "build", 0)) }(), 3.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        // "start" + 3x "iter" = 4 allocs, all freed (3 on reassign + 1 at scope exit)
        assert_eq!(allocs, 4, "4 strings allocated");
        assert_eq!(allocs, frees, "loop reassign: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_closure_strings_freed, {
        // Closure that creates strings — all should be freed at scope exit
        let mut m = jit(r#"
            pub fn closure_test() -> Number {
                const greeting = "hello"
                const unused = "waste"
                return 42
            }
        "#);
        runtime::MEM.reset();
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "closure_test", 0)) };
        assert_eq!(f(), 42.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert_eq!(allocs, frees, "closure context strings freed: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_closure_as_value_no_leak, {
        // First-class closure — the closure pointer itself isn't heap-allocated
        // but strings created inside the closure should be freed
        let mut m = jit(r#"
            pub fn use_closure() -> Number {
                const double = fn(x) -> x * 2
                const temp = "some_string"
                return double(5)
            }
        "#);
        runtime::MEM.reset();
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "use_closure", 0)) };
        assert_eq!(f(), 10.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert_eq!(allocs, frees, "closure value no leak: {} allocs, {} frees", allocs, frees);
    });

    mem_test!(mem_closure_passed_as_arg_no_leak, {
        // Closure passed to another function — strings in caller freed
        let mut m = jit(r#"
            pub fn apply(n: Number, transform: fn(Number) -> Number) -> Number {
                return transform(n)
            }
            pub fn caller() -> Number {
                const label = "tracking"
                const triple = fn(x) -> x * 3
                return apply(4, triple)
            }
        "#);
        runtime::MEM.reset();
        let f = unsafe { std::mem::transmute::<_, fn() -> f64>(call_f64(&mut m, "caller", 0)) };
        assert_eq!(f(), 12.0);
        let (allocs, frees, _, _, _) = runtime::MEM.stats();
        assert_eq!(allocs, frees, "closure arg no leak: {} allocs, {} frees", allocs, frees);
    });
}
