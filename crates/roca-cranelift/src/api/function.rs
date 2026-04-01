//! Function builder — describes and compiles a Roca function.
//! Handles all boilerplate: signature, params, constraints, cleanup, default return.

use std::collections::HashMap;
use cranelift_codegen::ir::{types, Value};
use cranelift_module::{Module, FuncId};

use roca_ast::crash::CrashHandlerKind;
use roca_types::RocaType;
use crate::builder::{FunctionCompiler, FunctionSpec, ParamSpec};
use crate::context::{CompiledFuncs, EmitCtx, StructLayout};
use crate::cranelift_type::CraneliftType;
use crate::registry::RuntimeFuncs;
use crate::helpers::default_for_ir_type;
use crate::emit_helpers::emit_scope_cleanup;
use super::body::Body;

/// Roca function builder. Describe the function, then compile it.
pub struct Function {
    pub name: String,
    pub params: Vec<ParamSpec>,
    pub constraints: Vec<Vec<roca_ast::Constraint>>,
    pub return_type: RocaType,
    pub returns_err: bool,
    pub self_param: bool,
    pub crash_handlers: HashMap<String, CrashHandlerKind>,
    pub struct_layouts: HashMap<String, StructLayout>,
    pub var_struct_type: HashMap<String, String>,
    pub func_return_kinds: HashMap<String, RocaType>,
    pub enum_variants: HashMap<String, Vec<String>>,
    pub struct_defs: HashMap<String, Vec<roca_ast::Field>>,
}

impl Function {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            params: Vec::new(),
            constraints: Vec::new(),
            return_type: RocaType::Void,
            returns_err: false,
            self_param: false,
            crash_handlers: HashMap::new(),
            struct_layouts: HashMap::new(),
            var_struct_type: HashMap::new(),
            func_return_kinds: HashMap::new(),
            enum_variants: HashMap::new(),
            struct_defs: HashMap::new(),
        }
    }

    pub fn param(mut self, name: &str, roca_type: RocaType) -> Self {
        self.params.push(ParamSpec { name: name.to_string(), roca_type });
        self.constraints.push(Vec::new());
        self
    }

    pub fn param_with_constraints(mut self, name: &str, roca_type: RocaType, constraints: Vec<roca_ast::Constraint>) -> Self {
        self.params.push(ParamSpec { name: name.to_string(), roca_type });
        self.constraints.push(constraints);
        self
    }

    pub fn returns(mut self, roca_type: RocaType) -> Self {
        self.return_type = roca_type;
        self
    }

    pub fn returns_err(mut self) -> Self {
        self.returns_err = true;
        self
    }

    pub fn self_param(mut self) -> Self {
        self.self_param = true;
        self
    }

    pub fn crash(mut self, call: &str, strategy: &CrashHandlerKind) -> Self {
        self.crash_handlers.insert(call.to_string(), strategy.clone());
        self
    }

    pub fn with_return_kinds(mut self, kinds: HashMap<String, RocaType>) -> Self {
        self.func_return_kinds = kinds;
        self
    }

    pub fn with_enum_variants(mut self, variants: HashMap<String, Vec<String>>) -> Self {
        self.enum_variants = variants;
        self
    }

    pub fn with_struct_defs(mut self, defs: HashMap<String, Vec<roca_ast::Field>>) -> Self {
        self.struct_defs = defs;
        self
    }

    pub fn with_struct_layout(mut self, name: &str, layout: StructLayout) -> Self {
        self.struct_layouts.insert(name.to_string(), layout);
        self
    }

    pub fn with_self_struct_type(mut self, struct_name: &str) -> Self {
        self.var_struct_type.insert("self".to_string(), struct_name.to_string());
        self
    }

    /// Compile the function. The body callback receives a `Body` to emit Roca code into.
    pub fn build<M: Module>(
        self,
        module: &mut M,
        rt: &RuntimeFuncs,
        compiled: &mut CompiledFuncs,
        body_fn: impl FnOnce(&mut Body),
    ) -> Result<FuncId, String> {
        let spec = FunctionSpec {
            name: self.name.clone(),
            params: self.params,
            self_param: self.self_param,
            return_type: self.return_type.clone(),
            returns_err: self.returns_err,
        };

        let return_type = self.return_type.clone();
        let returns_err = self.returns_err;
        let crash_handlers = self.crash_handlers;
        let struct_layouts = self.struct_layouts;
        let var_struct_type = self.var_struct_type;
        let func_return_kinds = self.func_return_kinds;
        let enum_variants = self.enum_variants;
        let struct_defs = self.struct_defs;
        let constraints = self.constraints;
        let self_param = self.self_param;

        FunctionCompiler::compile(module, &spec, rt, compiled, |ir, func_refs, block_params| {
            let ret_cl_type = return_type.to_cranelift();

            let mut ctx = EmitCtx {
                vars: HashMap::new(),
                func_refs,
                returns_err,
                return_type: ret_cl_type,
                struct_layouts,
                var_struct_type,
                crash_handlers,
                func_return_kinds,
                enum_variants,
                struct_defs,
                live_heap_vars: Vec::new(),
                loop_heap_base: 0,
                loop_exit: None,
                loop_header: None,
            };

            // Store params into slots automatically
            let param_offset = if self_param { 1 } else { 0 };
            if self_param && !block_params.is_empty() {
                let self_slot = ir.alloc_var(block_params[0]);
                ctx.vars.insert("self".to_string(), crate::context::VarInfo {
                    slot: self_slot.0,
                    cranelift_type: types::I64,
                    kind: RocaType::Struct("".into()),
                    is_heap: false, // self is borrowed, not owned
                });
            }
            for (i, p) in spec.params.iter().enumerate() {
                let cl_type = p.roca_type.to_cranelift();
                let slot = ir.alloc_var(block_params[i + param_offset]);
                // Params are borrowed, not owned — don't add to live_heap_vars
                ctx.vars.insert(p.name.clone(), crate::context::VarInfo {
                    slot: slot.0,
                    cranelift_type: cl_type,
                    kind: p.roca_type.clone(),
                    is_heap: false,
                });
            }

            // TODO: emit_param_constraints here using constraints vec

            let mut body = Body { ir: &mut *ir, ctx, returned: false };

            // Run the user's body
            body_fn(&mut body);

            // Auto default return if body didn't return
            if !body.returned {
                emit_scope_cleanup(&mut body.ir, &body.ctx, None);
                let default_val = default_for_ir_type(body.ir.raw(), ret_cl_type);
                if returns_err {
                    let no_err = body.ir.const_bool(false);
                    body.ir.ret_with_err(default_val, no_err);
                } else {
                    body.ir.ret(default_val);
                }
            }
        })
    }
}
