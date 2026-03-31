//! Native engine — Cranelift JIT compilation for proof tests and native execution.

pub mod types;
pub mod helpers;
pub mod runtime;
pub mod emit;
#[allow(dead_code)]
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
pub fn compile_all<M: Module>(
    module: &mut M,
    source: &crate::ast::SourceFile,
) -> Result<(), String> {
    let rt = runtime::declare_runtime(module);
    let mut compiled = emit::CompiledFuncs::new();

    let func_return_kinds = emit::build_return_kind_map(source);
    let enum_variants = emit::build_enum_variant_map(source);
    let struct_defs = emit::build_struct_def_map(source);

    emit::declare_all_functions(module, source, &mut compiled)?;

    for item in &source.items {
        match item {
            crate::ast::Item::ExternFn(ef) => {
                let mock = crate::emit::test_harness::values::auto_mock_def_for_extern_fn(ef);
                emit::compile_mock_stub(module, ef, &mock, &rt, &mut compiled)?;
            }
            crate::ast::Item::ExternContract(c) => {
                emit::compile_contract_stubs(module, c, &rt, &mut compiled)?;
            }
            _ => {}
        }
    }

    emit::compile_closures(module, source, &rt, &mut compiled, &func_return_kinds)?;
    emit::compile_wait_exprs(module, source, &rt, &mut compiled, &func_return_kinds)?;

    for item in &source.items {
        match item {
            crate::ast::Item::Function(f) => {
                emit::compile_function(module, f, &rt, &mut compiled, &func_return_kinds, &enum_variants, &struct_defs)?;
            }
            crate::ast::Item::Struct(s) => {
                for method in &s.methods {
                    emit::compile_struct_method(module, method, &s.name, &s.fields, &rt, &mut compiled, &func_return_kinds, &enum_variants, &struct_defs)?;
                }
            }
            crate::ast::Item::Satisfies(sat) => {
                for method in &sat.methods {
                    emit::compile_struct_method(module, method, &sat.struct_name, &[], &rt, &mut compiled, &func_return_kinds, &enum_variants, &struct_defs)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Compile Roca source to an object file via Cranelift AOT (production).
#[allow(dead_code)]
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

// Test modules — each in its own file under 500 lines
#[cfg(test)] mod tests_basic;
#[cfg(test)] mod tests_control;
#[cfg(test)] mod tests_features;
#[cfg(test)] mod tests_stdlib;
#[cfg(test)] mod tests_stdlib_ext;
#[cfg(test)] mod tests_memory;
#[cfg(test)] mod tests_memory_complex;
#[cfg(test)] mod tests_integration;
#[cfg(test)] mod tests_memory_stdlib;
