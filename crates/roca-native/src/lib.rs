//! Native compiler backend — Cranelift JIT compilation for proof tests and
//! optional AOT object-file emission.
//!
//! Depends on [`roca_ast`], [`roca_types`], [`roca_cranelift`] (IR builder),
//! and [`roca_runtime`] (host functions). Consumed by `roca-cli` to execute
//! inline `test {}` blocks before emitting JavaScript.
//!
//! # Key exports
//!
//! - [`compile_all()`] — compile every function, struct method, and satisfies
//!   method in a [`roca_ast::SourceFile`] into a Cranelift module.
//! - [`create_jit_module()`] — create a `JITModule` with all runtime symbols
//!   pre-registered.
//! - [`get_function_ptr()`] — look up a compiled function by name.
//! - [`compile_to_object()`] — AOT path that produces a relocatable object file.
//! - [`test_runner`] — executes proof tests against a finalized JIT module and
//!   reports pass/fail results.
//! - [`property_tests`] — fuzz-based property testing driven by parameter
//!   constraints.

pub mod runtime;
pub mod emit;
pub mod test_runner;
pub mod property_tests;
#[cfg(test)]
mod test_helpers;
#[cfg(test)]
mod tests_stdlib_integration;

use roca_ast as ast;
use cranelift_jit::{JITBuilder, JITModule};

fn default_expr_for_type(ty: &ast::TypeRef) -> ast::Expr {
    match ty {
        ast::TypeRef::String => ast::Expr::String("".into()),
        ast::TypeRef::Number => ast::Expr::Number(0.0),
        ast::TypeRef::Bool => ast::Expr::Bool(false),
        ast::TypeRef::Ok => ast::Expr::Null,
        ast::TypeRef::Generic(name, _) if name == "Array" => ast::Expr::Array(vec![]),
        _ => ast::Expr::Null,
    }
}
use cranelift_module::Module;
use cranelift_object::{ObjectBuilder, ObjectModule};

/// Create a JIT module with the Roca runtime functions registered.
pub fn create_jit_module() -> JITModule {
    let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
        .expect("failed to create JIT builder");
    runtime::register_symbols(&mut builder);
    JITModule::new(builder)
}

/// Look up a compiled function by name and return its native pointer.
/// Returns None if the function wasn't compiled.
pub fn get_function_ptr(module: &JITModule, name: &str) -> Option<*const u8> {
    let id = match module.get_name(name) {
        Some(cranelift_module::FuncOrDataId::Func(id)) => id,
        _ => return None,
    };
    Some(module.get_finalized_function(id))
}

/// Compile all functions in a source file into a module.
pub fn compile_all<M: Module>(
    module: &mut M,
    source: &roca_ast::SourceFile,
) -> Result<(), String> {
    let rt = runtime::declare_runtime(module);
    let mut compiled = emit::CompiledFuncs::new();

    let func_return_kinds = emit::build_return_kind_map(source);
    let enum_variants = emit::build_enum_variant_map(source);
    let struct_defs = emit::build_struct_def_map(source);

    emit::declare_all_functions(module, source, &mut compiled)?;

    for item in &source.items {
        match item {
            roca_ast::Item::ExternFn(ef) => {
                let default_value = default_expr_for_type(&ef.return_type);
                emit::compile_extern_fn_stub(module, ef, &default_value, &rt, &mut compiled)?;
            }
            roca_ast::Item::ExternContract(c) => {
                emit::compile_contract_stubs(module, c, &rt, &mut compiled)?;
            }
            _ => {}
        }
    }

    emit::compile_closures(module, source, &rt, &mut compiled, &func_return_kinds)?;
    emit::compile_wait_exprs(module, source, &rt, &mut compiled, &func_return_kinds)?;

    for item in &source.items {
        match item {
            roca_ast::Item::Function(f) => {
                emit::compile_function(module, f, &rt, &mut compiled, &func_return_kinds, &enum_variants, &struct_defs)?;
            }
            roca_ast::Item::Struct(s) => {
                for method in &s.methods {
                    emit::compile_struct_method(module, method, &s.name, &s.fields, &rt, &mut compiled, &func_return_kinds, &enum_variants, &struct_defs)?;
                }
            }
            roca_ast::Item::Satisfies(sat) => {
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
pub fn compile_to_object(source: &roca_ast::SourceFile) -> Result<Vec<u8>, String> {
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
