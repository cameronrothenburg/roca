//! Roca AST → native code emission via roca-cranelift builders.

pub mod compile;
pub mod context;
pub mod emit;

pub use roca_cranelift::CompiledFuncs;
pub use compile::{
    build_return_kind_map, build_enum_variant_map, build_struct_def_map,
    declare_all_functions, compile_closures, compile_function,
    compile_struct_method, compile_extern_fn_stub, compile_contract_stubs, compile_wait_exprs,
};
