//! Function builder — describes and compiles a Roca function.
//! Handles all boilerplate: signature, params, constraints, cleanup, default return.

use std::collections::HashMap;
use cranelift_codegen::ir::types;
use cranelift_module::{Module, FuncId};

use roca_ast::crash::CrashHandlerKind;
use roca_types::{self as rt, RocaType, Param, Field};
use crate::builder::{FunctionCompiler, FunctionSpec, ParamSpec};
use crate::context::{CompiledFuncs, EmitCtx, StructLayout, VarInfo};
use crate::cranelift_type::CraneliftType;
use crate::registry::RuntimeFuncs;
use crate::helpers::default_for_ir_type;
use crate::emit_helpers::emit_scope_cleanup;
use super::body::Body;

// ─── Function ─────────────────────────────────────────

/// Roca function builder. Describe the function, then compile it.
pub struct Function {
    name: String,
    params: Vec<Param>,
    return_type: RocaType,
    returns_err: bool,
    self_param: bool,
    crash_handlers: HashMap<String, CrashHandlerKind>,
    struct_layouts: HashMap<String, StructLayout>,
    var_struct_type: HashMap<String, String>,
    func_return_kinds: HashMap<String, RocaType>,
    enum_variants: HashMap<String, Vec<String>>,
    struct_defs: HashMap<String, Vec<roca_ast::Field>>,
}

impl Function {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            params: Vec::new(),
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

    /// Set params from roca-types Param array.
    pub fn params(mut self, params: &[Param]) -> Self {
        self.params = params.to_vec();
        self
    }

    /// Add a single parameter by name and type.
    pub fn param(mut self, name: &str, roca_type: RocaType) -> Self {
        self.params.push(Param { name: name.to_string(), roca_type, constraints: Vec::new() });
        self
    }

    /// Add a single parameter with constraints.
    pub fn param_with_constraints(mut self, name: &str, roca_type: RocaType, constraints: Vec<roca_ast::Constraint>) -> Self {
        let converted: Vec<rt::Constraint> = constraints.iter().map(rt::Constraint::from).collect();
        self.params.push(Param { name: name.to_string(), roca_type, constraints: converted });
        self
    }

    /// Set return type.
    pub fn returns(mut self, roca_type: RocaType) -> Self {
        self.return_type = roca_type;
        self
    }

    /// Mark as returning errors.
    pub fn returns_err(mut self) -> Self {
        self.returns_err = true;
        self
    }

    /// Conditionally mark as returning errors.
    pub fn returns_err_if(mut self, flag: bool) -> Self {
        self.returns_err = flag;
        self
    }

    /// Mark as a struct method (adds self as first param).
    pub fn self_param(mut self) -> Self {
        self.self_param = true;
        self
    }

    /// Set crash block — registers all handlers.
    pub fn crash(mut self, crash: &roca_ast::CrashBlock) -> Self {
        for h in &crash.handlers {
            self.crash_handlers.insert(h.call.clone(), h.strategy.clone());
        }
        self
    }

    /// Optionally set crash block.
    pub fn crash_opt(self, crash: Option<&roca_ast::CrashBlock>) -> Self {
        if let Some(c) = crash { self.crash(c) } else { self }
    }

    /// Register a single crash handler for a call.
    pub fn crash_handler(mut self, call_name: &str, strategy: &CrashHandlerKind) -> Self {
        self.crash_handlers.insert(call_name.to_string(), strategy.clone());
        self
    }

    pub fn with_return_kinds(mut self, kinds: HashMap<String, RocaType>) -> Self {
        self.func_return_kinds = kinds; self
    }

    pub fn with_enum_variants(mut self, variants: HashMap<String, Vec<String>>) -> Self {
        self.enum_variants = variants; self
    }

    pub fn with_struct_defs(mut self, defs: HashMap<String, Vec<roca_ast::Field>>) -> Self {
        self.struct_defs = defs; self
    }

    pub fn with_struct_layout(mut self, name: &str, layout: StructLayout) -> Self {
        self.struct_layouts.insert(name.to_string(), layout); self
    }

    pub fn with_self_struct_type(mut self, struct_name: &str) -> Self {
        self.var_struct_type.insert("self".to_string(), struct_name.to_string()); self
    }

    /// Compile the function. Body callback receives a `Body` to emit Roca code into.
    pub fn build<M: Module>(
        self,
        module: &mut M,
        rt_funcs: &RuntimeFuncs,
        compiled: &mut CompiledFuncs,
        body_fn: impl FnOnce(&mut Body),
    ) -> Result<FuncId, String> {
        let spec = FunctionSpec {
            name: self.name.clone(),
            params: self.params.iter().map(|p| ParamSpec {
                name: p.name.clone(),
                roca_type: p.roca_type.clone(),
            }).collect(),
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
        let self_param = self.self_param;
        let params = self.params;

        FunctionCompiler::compile(module, &spec, rt_funcs, compiled, |ir, func_refs, block_params| {
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

            // Store params automatically
            let param_offset = if self_param { 1 } else { 0 };
            if self_param && !block_params.is_empty() {
                ctx.vars.insert("self".to_string(), VarInfo {
                    slot: ir.alloc_var(block_params[0]).0,
                    cranelift_type: types::I64,
                    kind: RocaType::Struct("".into()),
                    is_heap: false,
                });
            }
            for (i, p) in params.iter().enumerate() {
                let cl_type = p.roca_type.to_cranelift();
                let slot = ir.alloc_var(block_params[i + param_offset]);
                ctx.vars.insert(p.name.clone(), VarInfo {
                    slot: slot.0,
                    cranelift_type: cl_type,
                    kind: p.roca_type.clone(),
                    is_heap: false, // params are borrowed
                });
            }

            // TODO: emit_param_constraints

            let mut body = Body { ir: &mut *ir, ctx, returned: false };
            body_fn(&mut body);

            // Auto default return
            if !body.returned {
                emit_scope_cleanup(&mut *body.ir, &body.ctx, None);
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

// ─── Struct ───────────────────────────────────────────

/// Method definition for a struct.
pub struct Method {
    name: String,
    params: Vec<Param>,
    return_type: RocaType,
    returns_err: bool,
    crash_handlers: HashMap<String, CrashHandlerKind>,
    body_fn: Option<Box<dyn FnOnce(&mut Body)>>,
}

impl Method {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            params: Vec::new(),
            return_type: RocaType::Void,
            returns_err: false,
            crash_handlers: HashMap::new(),
            body_fn: None,
        }
    }

    pub fn params(mut self, params: &[Param]) -> Self {
        self.params = params.to_vec(); self
    }

    pub fn returns(mut self, roca_type: RocaType) -> Self {
        self.return_type = roca_type; self
    }

    pub fn returns_err(mut self) -> Self {
        self.returns_err = true; self
    }

    pub fn crash(mut self, crash: &roca_ast::CrashBlock) -> Self {
        for h in &crash.handlers {
            self.crash_handlers.insert(h.call.clone(), h.strategy.clone());
        }
        self
    }

    pub fn body(mut self, f: impl FnOnce(&mut Body) + 'static) -> Self {
        self.body_fn = Some(Box::new(f)); self
    }
}

/// Roca struct builder.
pub struct Struct {
    name: String,
    fields: Vec<Field>,
    methods: Vec<Method>,
}

impl Struct {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), fields: Vec::new(), methods: Vec::new() }
    }

    pub fn fields(mut self, fields: &[Field]) -> Self {
        self.fields = fields.to_vec(); self
    }

    pub fn method(mut self, method: Method) -> Self {
        self.methods.push(method); self
    }

    pub fn build<M: Module>(
        self,
        module: &mut M,
        rt_funcs: &RuntimeFuncs,
        compiled: &mut CompiledFuncs,
        func_return_kinds: &HashMap<String, RocaType>,
        enum_variants: &HashMap<String, Vec<String>>,
        struct_defs: &HashMap<String, Vec<roca_ast::Field>>,
    ) -> Result<(), String> {
        let field_info: Vec<(String, RocaType)> = self.fields.iter()
            .map(|f| (f.name.clone(), f.roca_type.clone()))
            .collect();
        let layout = StructLayout { fields: field_info };

        for method in self.methods {
            let qualified = format!("{}.{}", self.name, method.name);
            let mut func = Function::new(&qualified)
                .params(&method.params)
                .returns(method.return_type)
                .returns_err_if(method.returns_err)
                .self_param()
                .with_struct_layout(&self.name, layout.clone())
                .with_self_struct_type(&self.name)
                .with_return_kinds(func_return_kinds.clone())
                .with_enum_variants(enum_variants.clone())
                .with_struct_defs(struct_defs.clone());

            for (call, strategy) in &method.crash_handlers {
                func.crash_handlers.insert(call.clone(), strategy.clone());
            }

            if let Some(body_fn) = method.body_fn {
                func.build(module, rt_funcs, compiled, body_fn)?;
            }
        }
        Ok(())
    }
}

// ─── Satisfies ────────────────────────────────────────

/// Roca satisfies builder — implements a contract on a struct.
pub struct Satisfies {
    struct_name: String,
    _contract_name: String,
    methods: Vec<Method>,
}

impl Satisfies {
    pub fn new(struct_name: &str, contract_name: &str) -> Self {
        Self { struct_name: struct_name.to_string(), _contract_name: contract_name.to_string(), methods: Vec::new() }
    }

    pub fn method(mut self, method: Method) -> Self {
        self.methods.push(method); self
    }

    pub fn build<M: Module>(
        self,
        module: &mut M,
        rt_funcs: &RuntimeFuncs,
        compiled: &mut CompiledFuncs,
        func_return_kinds: &HashMap<String, RocaType>,
        enum_variants: &HashMap<String, Vec<String>>,
        struct_defs: &HashMap<String, Vec<roca_ast::Field>>,
        struct_fields: Option<&[(String, RocaType)]>,
    ) -> Result<(), String> {
        for method in self.methods {
            let qualified = format!("{}.{}", self.struct_name, method.name);
            let mut func = Function::new(&qualified)
                .params(&method.params)
                .returns(method.return_type)
                .returns_err_if(method.returns_err)
                .self_param()
                .with_return_kinds(func_return_kinds.clone())
                .with_enum_variants(enum_variants.clone())
                .with_struct_defs(struct_defs.clone());

            if let Some(fields) = struct_fields {
                let layout = crate::StructLayout { fields: fields.to_vec() };
                func = func.with_struct_layout(&self.struct_name, layout)
                    .with_self_struct_type(&self.struct_name);
            }

            if let Some(body_fn) = method.body_fn {
                func.build(module, rt_funcs, compiled, body_fn)?;
            }
        }
        Ok(())
    }
}

// ─── Enum ─────────────────────────────────────────────

/// Roca enum builder — registers variants. No code emission (metadata only).
pub struct RocaEnum {
    pub name: String,
    pub variants: Vec<(String, Vec<RocaType>)>,
}

impl RocaEnum {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), variants: Vec::new() }
    }

    pub fn variant(mut self, name: &str, fields: &[RocaType]) -> Self {
        self.variants.push((name.to_string(), fields.to_vec())); self
    }
}

// ─── ExternFn ─────────────────────────────────────────

/// Extern function builder — generates a stub returning default value.
pub struct ExternFn {
    name: String,
    params: Vec<Param>,
    return_type: RocaType,
    returns_err: bool,
}

impl ExternFn {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), params: Vec::new(), return_type: RocaType::Void, returns_err: false }
    }

    pub fn params(mut self, params: &[Param]) -> Self { self.params = params.to_vec(); self }
    pub fn returns(mut self, ty: RocaType) -> Self { self.return_type = ty; self }
    pub fn returns_err(mut self) -> Self { self.returns_err = true; self }

    pub fn build<M: Module>(
        self,
        module: &mut M,
        rt_funcs: &RuntimeFuncs,
        compiled: &mut CompiledFuncs,
    ) -> Result<FuncId, String> {
        let return_type = self.return_type.clone();

        Function::new(&self.name)
            .params(&self.params)
            .returns(self.return_type)
            .returns_err_if(self.returns_err)
            .build(module, rt_funcs, compiled, |body| {
                // Extern stub: just return default value
                let dv = body.default_for(&return_type); body.return_val(dv);
            })
    }
}

// ─── ExternContract ───────────────────────────────────

/// Extern contract builder — generates stubs for all methods.
pub struct ExternContract {
    name: String,
    methods: Vec<(String, Vec<RocaType>, RocaType, bool)>,
}

impl ExternContract {
    pub fn new(name: &str) -> Self {
        Self { name: name.to_string(), methods: Vec::new() }
    }

    pub fn method(mut self, name: &str, param_types: &[RocaType], return_type: RocaType, returns_err: bool) -> Self {
        self.methods.push((name.to_string(), param_types.to_vec(), return_type, returns_err));
        self
    }

    pub fn build<M: Module>(
        self,
        module: &mut M,
        rt_funcs: &RuntimeFuncs,
        compiled: &mut CompiledFuncs,
    ) -> Result<(), String> {
        for (method_name, param_types, return_type, returns_err) in &self.methods {
            let qualified = format!("{}.{}", self.name, method_name);
            let params: Vec<Param> = param_types.iter().enumerate()
                .map(|(i, t)| Param { name: format!("p{}", i), roca_type: t.clone(), constraints: Vec::new() })
                .collect();
            let rt_clone = return_type.clone();

            Function::new(&qualified)
                .params(&params)
                .returns(return_type.clone())
                .returns_err_if(*returns_err)
                .build(module, rt_funcs, compiled, |body| {
                    let dv = body.default_for(&rt_clone); body.return_val(dv);
                })?;
        }
        Ok(())
    }
}
