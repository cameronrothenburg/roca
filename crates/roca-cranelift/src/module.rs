//! Module wrappers — hide raw Cranelift module types from consuming crates.

use std::ops::{Deref, DerefMut};
use cranelift_codegen::ir::AbiParam;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Module, Linkage, FuncOrDataId};

use roca_types::RocaType;
use crate::cranelift_type::CraneliftType;
use crate::context::CompiledFuncs;

/// JIT compilation module with runtime symbols pre-registered.
pub struct JitModule {
    inner: JITModule,
}

impl JitModule {
    /// Create a JIT module. `register_symbols` wires runtime host functions
    /// into the JIT builder before the module is created.
    pub fn new(register_symbols: impl FnOnce(&mut JITBuilder)) -> Self {
        let mut builder = JITBuilder::new(cranelift_module::default_libcall_names())
            .expect("failed to create JIT builder");
        register_symbols(&mut builder);
        Self { inner: JITModule::new(builder) }
    }

    /// Finalize all compiled function definitions. Must be called before
    /// looking up function pointers.
    pub fn finalize(&mut self) -> Result<(), String> {
        self.inner.finalize_definitions()
            .map_err(|e| format!("JIT finalize failed (ensure execheap permission is set): {}", e))
    }

    /// Look up a compiled function by name and return its native pointer.
    pub fn get_function_ptr(&self, name: &str) -> Option<*const u8> {
        let id = match self.inner.get_name(name) {
            Some(FuncOrDataId::Func(id)) => id,
            _ => return None,
        };
        Some(self.inner.get_finalized_function(id))
    }
}

impl Deref for JitModule {
    type Target = JITModule;
    fn deref(&self) -> &JITModule { &self.inner }
}

impl DerefMut for JitModule {
    fn deref_mut(&mut self) -> &mut JITModule { &mut self.inner }
}

/// A function declaration for forward references.
pub struct FnDecl {
    pub name: String,
    pub params: Vec<RocaType>,
    pub has_self: bool,
    pub return_type: RocaType,
    pub returns_err: bool,
}

/// Declare all functions in a module for forward references.
/// This enables any function to call any other.
pub fn declare_functions<M: Module>(
    module: &mut M,
    declarations: &[FnDecl],
    compiled: &mut CompiledFuncs,
) -> Result<(), String> {
    for decl in declarations {
        if compiled.funcs.contains_key(&decl.name) { continue; }
        let mut sig = module.make_signature();
        if decl.has_self {
            sig.params.push(AbiParam::new(cranelift_codegen::ir::types::I64));
        }
        for param_type in &decl.params {
            sig.params.push(AbiParam::new(param_type.to_cranelift()));
        }
        sig.returns.push(AbiParam::new(decl.return_type.to_cranelift()));
        if decl.returns_err {
            sig.returns.push(AbiParam::new(cranelift_codegen::ir::types::I8));
        }
        let func_id = module.declare_function(&decl.name, Linkage::Export, &sig)
            .map_err(|e| format!("declare {}: {}", decl.name, e))?;
        compiled.funcs.insert(decl.name.clone(), func_id);
    }
    Ok(())
}
