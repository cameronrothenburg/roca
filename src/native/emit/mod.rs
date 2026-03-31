//! Roca AST → Cranelift IR emission.

pub mod context;
pub mod compile;
pub mod helpers;
pub mod stmt;
pub mod expr;
pub mod methods;

// Re-export public API used by native/mod.rs
pub use context::CompiledFuncs;
pub use compile::{
    build_return_kind_map, build_enum_variant_map, build_struct_def_map,
    declare_all_functions, compile_closures, compile_function,
    compile_struct_method, compile_mock_stub, compile_contract_stubs, compile_wait_exprs,
};
