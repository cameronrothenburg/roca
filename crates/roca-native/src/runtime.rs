//! runtime.rs — registers roca-mem extern "C" functions into the JIT.
//!
//! Two phases:
//! 1. `register_symbols()` — called on JITBuilder before module creation
//! 2. `declare_all()` — called on JITModule to declare function signatures

use cranelift_codegen::ir::AbiParam;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module};

use cranelift_codegen::ir::types::{F64, I64};

/// Register symbol addresses on the JIT builder so they can be resolved at link time.
pub fn register_symbols(builder: &mut JITBuilder) {
    builder.symbol("mem_string_new", roca_mem::mem_string_new as *const u8);
    builder.symbol("mem_struct_new", roca_mem::mem_struct_new as *const u8);
    builder.symbol("mem_struct_get_f64", roca_mem::mem_struct_get_f64 as *const u8);
    builder.symbol("mem_struct_set_f64", roca_mem::mem_struct_set_f64 as *const u8);
    builder.symbol("mem_struct_get_ptr", roca_mem::mem_struct_get_ptr as *const u8);
    builder.symbol("mem_struct_set_owned", roca_mem::mem_struct_set_owned as *const u8);
    builder.symbol("mem_free", roca_mem::mem_free as *const u8);
}

/// Declare function signatures on the module. Returns name → FuncId map.
pub fn declare_all(module: &mut JITModule) -> std::collections::HashMap<String, FuncId> {
    let mut ids = std::collections::HashMap::new();

    macro_rules! decl {
        ($name:expr, [$($p:expr),*], [$($r:expr),*]) => {{
            let mut sig = module.make_signature();
            $( sig.params.push(AbiParam::new($p)); )*
            $( sig.returns.push(AbiParam::new($r)); )*
            let id = module.declare_function($name, Linkage::Import, &sig).unwrap();
            ids.insert($name.to_string(), id);
        }};
    }

    decl!("mem_string_new",      [I64],           [I64]);
    decl!("mem_struct_new",      [I64, I64],      [I64]);
    decl!("mem_struct_get_f64",  [I64, I64],      [F64]);
    decl!("mem_struct_set_f64",  [I64, I64, F64], []);
    decl!("mem_struct_get_ptr",  [I64, I64],      [I64]);
    decl!("mem_struct_set_owned",[I64, I64, I64], []);
    decl!("mem_free",            [I64],           []);

    ids
}
