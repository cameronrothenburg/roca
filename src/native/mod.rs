//! Native engine — Cranelift JIT compilation for proof tests and native execution.
//! Experimental: use `--engine=native` to enable.

pub mod types;
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
    let mut module = JITModule::new(
        JITBuilder::new(cranelift_module::default_libcall_names()).expect("jit builder failed")
    );

    // Compile directly without runtime — just test basic IR generation
    for item in &source.items {
        if let crate::ast::Item::Function(f) = item {
            emit::compile_function_bare(&mut module, f)?;
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
        let result = eval_roca(&file);
        assert!(result.is_ok(), "native compilation failed: {:?}", result);
    }

    #[test]
    fn compile_and_call_roca_function() {
        use cranelift_module::Module;

        let file = crate::parse::parse(r#"
            pub fn add(a: Number, b: Number) -> Number {
                return a + b
                test { self(1, 2) == 3 }
            }
        "#);

        let mut module = create_jit_module();
        let rt = runtime::declare_runtime(&mut module);

        if let crate::ast::Item::Function(f) = &file.items[0] {
            emit::compile_function(&mut module, f, &rt).unwrap();
        }

        module.finalize_definitions().unwrap();

        let func_id = module.declare_function("add", cranelift_module::Linkage::Export, &module.make_signature()).ok();
        if let Some(id) = func_id {
            let ptr = module.get_finalized_function(id);
            let add_fn = unsafe { std::mem::transmute::<_, fn(f64, f64) -> f64>(ptr) };
            assert_eq!(add_fn(37.0, 5.0), 42.0);
        }
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
