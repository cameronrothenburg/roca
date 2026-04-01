//! FunctionCompiler — handles the boilerplate of compiling a Roca function.
//! Signature building, module declaration, entry block, finalization.

use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, AbiParam, Value};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Module, Linkage, FuncId};
use roca_types::RocaType;
use crate::cranelift_type::CraneliftType;
use crate::context::CompiledFuncs;
use crate::registry::RuntimeFuncs;
use super::ir::IrBuilder;

/// Describes a parameter for function compilation.
pub struct ParamSpec {
    pub name: String,
    pub roca_type: RocaType,
}

/// Describes a function to be compiled.
pub struct FunctionSpec {
    pub name: String,
    pub params: Vec<ParamSpec>,
    /// Struct method: adds I64 self as first param.
    pub self_param: bool,
    pub return_type: RocaType,
    pub returns_err: bool,
}

impl FunctionSpec {
    /// Build a spec from a Roca function definition.
    pub fn from_fn(name: &str, params: &[roca_ast::Param], return_type: &roca_ast::TypeRef, returns_err: bool) -> Self {
        Self {
            name: name.to_string(),
            params: params.iter().map(|p| ParamSpec {
                name: p.name.clone(),
                roca_type: RocaType::from(&p.type_ref),
            }).collect(),
            self_param: false,
            return_type: RocaType::from(return_type),
            returns_err,
        }
    }

    /// Build a spec for a struct method (adds self param).
    pub fn from_method(struct_name: &str, func_name: &str, params: &[roca_ast::Param], return_type: &roca_ast::TypeRef, returns_err: bool) -> Self {
        Self {
            name: format!("{}.{}", struct_name, func_name),
            params: params.iter().map(|p| ParamSpec {
                name: p.name.clone(),
                roca_type: RocaType::from(&p.type_ref),
            }).collect(),
            self_param: true,
            return_type: RocaType::from(return_type),
            returns_err,
        }
    }
}

/// Compiles a single Roca function. Handles all Cranelift boilerplate:
/// signature, declaration, context, entry block, finalization.
///
/// The `body` callback receives:
/// - `IrBuilder` — typed IR emission methods
/// - `HashMap<String, FuncRef>` — runtime + compiled function refs
/// - `&[Value]` — entry block parameters (self + params)
pub struct FunctionCompiler;

impl FunctionCompiler {
    pub fn compile<M: Module>(
        module: &mut M,
        spec: &FunctionSpec,
        rt: &RuntimeFuncs,
        compiled: &mut CompiledFuncs,
        body: impl FnOnce(&mut IrBuilder, HashMap<String, ir::FuncRef>, &[Value]),
    ) -> Result<FuncId, String> {
        // 1. Build signature
        let mut sig = module.make_signature();
        if spec.self_param {
            sig.params.push(AbiParam::new(types::I64));
        }
        for p in &spec.params {
            sig.params.push(AbiParam::new(p.roca_type.to_cranelift()));
        }
        sig.returns.push(AbiParam::new(spec.return_type.to_cranelift()));
        if spec.returns_err {
            sig.returns.push(AbiParam::new(types::I8));
        }

        // 2. Declare
        let func_id = module.declare_function(&spec.name, Linkage::Export, &sig)
            .map_err(|e| format!("declare {}: {}", spec.name, e))?;
        compiled.funcs.insert(spec.name.clone(), func_id);

        // 3. Context + builder
        let mut ctx = module.make_context();
        ctx.func.signature = sig;
        let mut bc = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut bc);

        // 4. Entry block
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        // 5. Import runtime + compiled functions
        let func_refs = rt.import_all(module, &mut builder.func, compiled);
        let block_params: Vec<Value> = builder.block_params(entry).to_vec();

        // 6. Invoke body callback
        {
            let mut ir = IrBuilder { b: &mut builder };
            body(&mut ir, func_refs, &block_params);
        }

        // 7. Finalize + define
        builder.finalize();
        module.define_function(func_id, &mut ctx)
            .map_err(|e| format!("compile {}: {}", spec.name, e))?;
        module.clear_context(&mut ctx);
        Ok(func_id)
    }
}
