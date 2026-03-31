//! Native engine — Cranelift JIT compilation for proof tests and native execution.
//! Experimental: use `--engine=native` to enable.

pub mod types;
#[allow(dead_code)]
pub mod helpers;
pub mod runtime;
pub mod emit;

use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::Module;

/// Create a JIT module with the Roca runtime functions registered.
pub fn create_jit_module() -> JITModule {
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
        .expect("failed to create JIT builder");

    // Register runtime functions
    runtime::register_symbols(&mut builder);

    JITModule::new(builder)
}

/// Compile and run a simple Roca expression, return the result as a string.
/// This is the entry point for `--engine=native` proof test execution.
pub fn eval_roca(source: &crate::ast::SourceFile) -> Result<String, String> {
    let mut module = create_jit_module();
    let rt = runtime::declare_runtime(&mut module);
    let mut compiled = emit::CompiledFuncs::new();

    for item in &source.items {
        if let crate::ast::Item::Function(f) = item {
            emit::compile_function(&mut module, f, &rt, &mut compiled)?;
        }
    }

    module.finalize_definitions()
        .map_err(|e| format!("finalize error: {}", e))?;

    Ok("native engine initialized".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::InstBuilder;

    #[test]
    fn native_engine_initializes() {
        let module = create_jit_module();
        // If we get here, Cranelift JIT initialized successfully
        drop(module);
    }

    #[test]
    fn compile_roca_function() {
        let file = crate::parse::parse(r#"
            pub fn answer() -> Number {
                return 42
            }
        "#);

        let mut module = JITModule::new(
            cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
                .expect("jit builder failed")
        );

        if let crate::ast::Item::Function(f) = &file.items[0] {
            let rt = runtime::declare_runtime(&mut module);
            let mut compiled = emit::CompiledFuncs::new();
            emit::compile_function(&mut module, f, &rt, &mut compiled).unwrap();
        }

        module.finalize_definitions().unwrap();

        // Call the compiled function
        let mut sig = module.make_signature();
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        let func_id = module.declare_function("answer", cranelift_module::Linkage::Export, &sig).unwrap();
        let ptr = module.get_finalized_function(func_id);
        let answer_fn = unsafe { std::mem::transmute::<_, fn() -> f64>(ptr) };
        assert_eq!(answer_fn(), 42.0, "Roca function should return 42.0 natively");
    }

    #[test]
    fn compile_roca_add() {
        let file = crate::parse::parse(r#"
            pub fn add(a: Number, b: Number) -> Number {
                return a + b
            }
        "#);

        let mut module = JITModule::new(
            cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
                .expect("jit builder failed")
        );

        if let crate::ast::Item::Function(f) = &file.items[0] {
            let rt = runtime::declare_runtime(&mut module);
            let mut compiled = emit::CompiledFuncs::new();
            emit::compile_function(&mut module, f, &rt, &mut compiled).unwrap();
        }
        module.finalize_definitions().unwrap();

        let mut sig = module.make_signature();
        sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        let func_id = module.declare_function("add", cranelift_module::Linkage::Export, &sig).unwrap();
        let ptr = module.get_finalized_function(func_id);
        let add_fn = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(ptr) };

        assert_eq!(add_fn(37.0, 5.0), 42.0);
        assert_eq!(add_fn(0.0, 0.0), 0.0);
        assert_eq!(add_fn(-10.0, 10.0), 0.0);
    }

    #[test]
    fn compile_roca_if_else() {
        let file = crate::parse::parse(r#"
            pub fn clamp(n: Number) -> Number {
                if n > 100 { return 100 }
                if n < 0 { return 0 }
                return n
            }
        "#);

        let mut module = JITModule::new(
            cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
                .expect("jit builder failed")
        );

        if let crate::ast::Item::Function(f) = &file.items[0] {
            let rt = runtime::declare_runtime(&mut module);
            let mut compiled = emit::CompiledFuncs::new();
            emit::compile_function(&mut module, f, &rt, &mut compiled).unwrap();
        }
        module.finalize_definitions().unwrap();

        let mut sig = module.make_signature();
        sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        let func_id = module.declare_function("clamp", cranelift_module::Linkage::Export, &sig).unwrap();
        let ptr = module.get_finalized_function(func_id);
        let clamp_fn = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(ptr) };

        assert_eq!(clamp_fn(50.0), 50.0, "50 should pass through");
        assert_eq!(clamp_fn(150.0), 100.0, "150 should clamp to 100");
        assert_eq!(clamp_fn(-10.0), 0.0, "-10 should clamp to 0");
    }

    #[test]
    fn compile_roca_mul() {
        let file = crate::parse::parse(r#"
            pub fn square(n: Number) -> Number {
                return n * n
            }
        "#);

        let mut module = JITModule::new(
            cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
                .expect("jit builder failed")
        );

        if let crate::ast::Item::Function(f) = &file.items[0] {
            let rt = runtime::declare_runtime(&mut module);
            let mut compiled = emit::CompiledFuncs::new();
            emit::compile_function(&mut module, f, &rt, &mut compiled).unwrap();
        }
        module.finalize_definitions().unwrap();

        let mut sig = module.make_signature();
        sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        let func_id = module.declare_function("square", cranelift_module::Linkage::Export, &sig).unwrap();
        let ptr = module.get_finalized_function(func_id);
        let square_fn = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(ptr) };

        assert_eq!(square_fn(5.0), 25.0);
        assert_eq!(square_fn(0.0), 0.0);
        assert_eq!(square_fn(-3.0), 9.0);
    }

    #[test]
    fn compile_roca_const_binding() {
        let file = crate::parse::parse(r#"
            pub fn double_add(a: Number, b: Number) -> Number {
                const sum = a + b
                return sum + sum
            }
        "#);

        let mut module = JITModule::new(
            cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
                .expect("jit builder failed")
        );

        if let crate::ast::Item::Function(f) = &file.items[0] {
            let rt = runtime::declare_runtime(&mut module);
            let mut compiled = emit::CompiledFuncs::new();
            emit::compile_function(&mut module, f, &rt, &mut compiled).unwrap();
        }
        module.finalize_definitions().unwrap();

        let mut sig = module.make_signature();
        sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        let func_id = module.declare_function("double_add", cranelift_module::Linkage::Export, &sig).unwrap();
        let ptr = module.get_finalized_function(func_id);
        let fn_ptr = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(ptr) };

        assert_eq!(fn_ptr(3.0, 4.0), 14.0); // (3+4) + (3+4) = 14
        assert_eq!(fn_ptr(0.0, 5.0), 10.0);
    }

    #[test]
    fn compile_roca_string_literal() {
        // Test that string literals compile — returns a pointer (non-zero)
        let file = crate::parse::parse(r#"
            pub fn greeting() -> String {
                return "hello"
            }
        "#);

        let mut module = JITModule::new(
            cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
                .expect("jit builder failed")
        );

        if let crate::ast::Item::Function(f) = &file.items[0] {
            let rt = runtime::declare_runtime(&mut module);
            let mut compiled = emit::CompiledFuncs::new();
            emit::compile_function(&mut module, f, &rt, &mut compiled).unwrap();
        }
        module.finalize_definitions().unwrap();

        let mut sig = module.make_signature();
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::I64));
        let func_id = module.declare_function("greeting", cranelift_module::Linkage::Export, &sig).unwrap();
        let ptr = module.get_finalized_function(func_id);
        let greeting_fn = unsafe { std::mem::transmute::<_, fn() -> *const u8>(ptr) };

        let result = greeting_fn();
        assert!(!result.is_null(), "string should return non-null pointer");
        let cstr = unsafe { std::ffi::CStr::from_ptr(result as *const i8) };
        assert_eq!(cstr.to_str().unwrap(), "hello");
    }

    #[test]
    fn compile_roca_function_calls() {
        // Two functions: double calls add
        let file = crate::parse::parse(r#"
            pub fn add(a: Number, b: Number) -> Number {
                return a + b
            }
            pub fn double(n: Number) -> Number {
                return add(n, n)
            }
        "#);

        let mut module = JITModule::new(
            cranelift_jit::JITBuilder::new(cranelift_module::default_libcall_names())
                .expect("jit builder failed")
        );
        let rt = runtime::declare_runtime(&mut module);
        let mut compiled = emit::CompiledFuncs::new();

        // Compile both functions
        for item in &file.items {
            if let crate::ast::Item::Function(f) = item {
                emit::compile_function(&mut module, f, &rt, &mut compiled).unwrap();
            }
        }
        module.finalize_definitions().unwrap();

        // Call double(5) — should call add(5, 5) = 10
        let mut sig = module.make_signature();
        sig.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        sig.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        let func_id = module.declare_function("double", cranelift_module::Linkage::Export, &sig).unwrap();
        let ptr = module.get_finalized_function(func_id);
        let double_fn = unsafe { std::mem::transmute::<_, fn(f64) -> f64>(ptr) };

        assert_eq!(double_fn(5.0), 10.0);
        assert_eq!(double_fn(21.0), 42.0);
    }

    #[test]
    fn compile_raw_cranelift() {
        // Test Cranelift directly — no Roca AST involved
        use cranelift_codegen::ir::{types, AbiParam};
        use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
        use cranelift_module::{Module, Linkage};

        let mut module = create_jit_module();
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));

        let func_id = module.declare_function("test_add", Linkage::Export, &sig).unwrap();
        let mut ctx = module.make_context();
        ctx.func.signature = sig;

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

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

        let code_ptr = module.get_finalized_function(func_id);
        let add_fn = unsafe { std::mem::transmute::<_, fn(i64, i64) -> i64>(code_ptr) };

        assert_eq!(add_fn(37, 5), 42);
        assert_eq!(add_fn(100, -58), 42);
    }
}
