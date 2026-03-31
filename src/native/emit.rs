//! Roca AST → Cranelift IR emission.

use std::collections::HashMap;
use cranelift_codegen::ir::{self, types, AbiParam, InstBuilder, Value, FuncRef, BlockArg};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Module, Linkage, FuncId};

use crate::ast::{self as roca, Expr, Stmt, BinOp, StringPart, crash::{CrashHandlerKind, CrashStep}};
use super::types::roca_to_cranelift;
use super::runtime::RuntimeFuncs;
use super::helpers::{
    fcmp_to_i64, icmp_to_i64, call_rt, call_void, alloc_slot, load_slot,
    bool_and, bool_or, ensure_i64, leak_cstr, default_for_ir_type,
};

/// Tracks compiled functions for cross-function references
pub struct CompiledFuncs {
    pub funcs: HashMap<String, FuncId>,
}

impl CompiledFuncs {
    pub fn new() -> Self { Self { funcs: HashMap::new() } }
}

#[derive(Clone)]
struct VarInfo {
    slot: ir::StackSlot,
    cranelift_type: ir::Type,
    kind: ValKind,
    is_heap: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum ValKind {
    Number,
    String,
    Bool,
    Array,
    Struct,
    /// Algebraic enum variant — tagged struct with string tag at slot 0
    EnumVariant,
    Other, // unknown — not freed at scope exit (safety: only free what we can identify)
}

/// Tracks struct field layouts for field access by index and type.
#[derive(Clone)]
struct StructLayout {
    fields: Vec<(String, ValKind)>,
}

impl StructLayout {
    fn field_index(&self, name: &str) -> Option<usize> {
        self.fields.iter().position(|(f, _)| f == name)
    }

    fn field_kind(&self, name: &str) -> ValKind {
        self.fields.iter().find(|(f, _)| f == name).map(|(_, k)| *k).unwrap_or(ValKind::Other)
    }
}

/// Everything needed during emission — avoids parameter sprawl
struct EmitCtx {
    vars: HashMap<String, VarInfo>,
    func_refs: HashMap<String, FuncRef>,
    returns_err: bool,
    return_type: ir::Type,
    struct_layouts: HashMap<String, StructLayout>,
    var_struct_type: HashMap<String, String>,
    crash_handlers: HashMap<String, CrashHandlerKind>,
    /// Function name → return kind (for tracking what kind of value a call produces)
    func_return_kinds: HashMap<String, ValKind>,
    /// Enum name → set of variant names (for recognizing Token.Plus as enum construction)
    enum_variants: HashMap<String, Vec<String>>,
    /// Struct name → field definitions (for constraint validation)
    struct_defs: HashMap<String, Vec<roca::Field>>,
    live_heap_vars: Vec<String>,
    loop_heap_base: usize,
    loop_exit: Option<ir::Block>,
    loop_header: Option<ir::Block>,
}

impl EmitCtx {
    fn get_var(&self, name: &str) -> Option<&VarInfo> {
        self.vars.get(name)
    }

    fn set_var(&mut self, name: String, slot: ir::StackSlot, ty: ir::Type) {
        let is_heap = ty == types::I64;
        let kind = match ty {
            t if t == types::F64 => ValKind::Number,
            t if t == types::I8 => ValKind::Bool,
            _ => ValKind::Other,
        };
        if is_heap && !self.live_heap_vars.contains(&name) {
            self.live_heap_vars.push(name.clone());
        }
        self.vars.insert(name, VarInfo { slot, cranelift_type: ty, kind, is_heap });
    }

    fn set_var_kind(&mut self, name: String, slot: ir::StackSlot, ty: ir::Type, kind: ValKind) {
        let is_heap = ty == types::I64;
        if is_heap && !self.live_heap_vars.contains(&name) {
            self.live_heap_vars.push(name.clone());
        }
        self.vars.insert(name, VarInfo { slot, cranelift_type: ty, kind, is_heap });
    }

    fn get_func(&self, name: &str) -> Option<&FuncRef> {
        self.func_refs.get(name)
    }
}

// ─── Compile ───────────────────────────────────────────

/// Build a map of function name → return ValKind from the source file.
pub fn build_return_kind_map(source: &roca::SourceFile) -> HashMap<String, ValKind> {
    let mut map = HashMap::new();
    for item in &source.items {
        match item {
            roca::Item::Function(f) => {
                map.insert(f.name.clone(), type_ref_to_kind(&f.return_type));
            }
            roca::Item::ExternFn(ef) => {
                map.insert(ef.name.clone(), type_ref_to_kind(&ef.return_type));
            }
            _ => {}
        }
    }
    map
}

/// Build a map of enum name → variant names from the source file.
pub fn build_enum_variant_map(source: &roca::SourceFile) -> HashMap<String, Vec<String>> {
    let mut map = HashMap::new();
    for item in &source.items {
        if let roca::Item::Enum(e) = item {
            if e.is_algebraic {
                let variants = e.variants.iter().map(|v| v.name.clone()).collect();
                map.insert(e.name.clone(), variants);
            }
        }
    }
    map
}

/// Build a map of struct name → field definitions from the source file.
pub fn build_struct_def_map(source: &roca::SourceFile) -> HashMap<String, Vec<roca::Field>> {
    let mut map = HashMap::new();
    for item in &source.items {
        if let roca::Item::Struct(s) = item {
            map.insert(s.name.clone(), s.fields.clone());
        }
    }
    map
}

/// Declare all functions in the module (signatures only, no bodies).
/// This enables forward references — any function can call any other.
pub fn declare_all_functions<M: Module>(
    module: &mut M,
    source: &roca::SourceFile,
    compiled: &mut CompiledFuncs,
) -> Result<(), String> {
    for item in &source.items {
        let fns_to_declare: Vec<(&roca::FnDef, Option<&str>)> = match item {
            roca::Item::Function(f) => vec![(f, None)],
            roca::Item::Struct(s) => s.methods.iter().map(|m| (m, Some(s.name.as_str()))).collect(),
            roca::Item::Satisfies(sat) => sat.methods.iter().map(|m| (m, Some(sat.struct_name.as_str()))).collect(),
            _ => vec![],
        };
        for (f, struct_name) in fns_to_declare {
            let qualified = if let Some(sn) = struct_name {
                format!("{}.{}", sn, f.name)
            } else {
                f.name.clone()
            };
            if compiled.funcs.contains_key(&qualified) { continue; }
            let mut sig = module.make_signature();
            // Struct methods get `self` (I64 struct pointer) as first param
            if struct_name.is_some() {
                sig.params.push(AbiParam::new(types::I64));
            }
            for param in &f.params {
                sig.params.push(AbiParam::new(roca_to_cranelift(&param.type_ref)));
            }
            sig.returns.push(AbiParam::new(roca_to_cranelift(&f.return_type)));
            if f.returns_err {
                sig.returns.push(AbiParam::new(types::I8));
            }
            let func_id = module.declare_function(&qualified, Linkage::Export, &sig)
                .map_err(|e| format!("declare {}: {}", qualified, e))?;
            compiled.funcs.insert(qualified, func_id);
        }
    }
    Ok(())
}

/// Pre-compile all closures in a source file as top-level functions.
/// Each closure gets a unique name based on its params and body hash.
pub fn compile_closures<M: Module>(
    module: &mut M,
    source: &roca::SourceFile,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    func_return_kinds: &HashMap<String, ValKind>,
) -> Result<(), String> {
    let mut closures = Vec::new();
    for item in &source.items {
        if let roca::Item::Function(f) = item {
            collect_closures(&f.body, &mut closures);
        }
    }
    for (params, body) in closures {
        let name = format!("__closure_{}_{}", params.len(), closure_hash(&params, &body));
        if compiled.funcs.contains_key(&name) { continue; }

        // Build signature: assume f64 params and f64 return for numeric closures
        // TODO: infer from fn(A) -> B type annotations
        let mut sig = module.make_signature();
        for _ in &params {
            sig.params.push(AbiParam::new(types::F64));
        }
        sig.returns.push(AbiParam::new(types::F64));

        let func_id = module.declare_function(&name, Linkage::Export, &sig)
            .map_err(|e| format!("declare closure: {}", e))?;
        compiled.funcs.insert(name.clone(), func_id);

        let mut ctx = module.make_context();
        ctx.func.signature = sig;
        let mut bc = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut bc);

        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let mut emit_ctx = EmitCtx {
            vars: HashMap::new(),
            func_refs: rt.import_all(module, &mut builder.func, compiled),
            returns_err: false,
            return_type: types::F64,
            struct_layouts: HashMap::new(),
            var_struct_type: HashMap::new(),
            crash_handlers: HashMap::new(),
            func_return_kinds: func_return_kinds.clone(),
            enum_variants: HashMap::new(),
        struct_defs: HashMap::new(),
            live_heap_vars: Vec::new(),
            loop_heap_base: 0,
            loop_exit: None,
            loop_header: None,
        };

        // Bind params
        let block_params: Vec<Value> = builder.block_params(entry).to_vec();
        for (i, p) in params.iter().enumerate() {
            let slot = alloc_slot(&mut builder, block_params[i]);
            emit_ctx.set_var_kind(p.clone(), slot, types::F64, ValKind::Number);
        }

        let val = emit_expr(&mut builder, &body, &mut emit_ctx);
        emit_scope_cleanup(&mut builder, &emit_ctx, None);
        builder.ins().return_(&[val]);
        builder.finalize();

        module.define_function(func_id, &mut ctx)
            .map_err(|e| format!("compile closure {}: {}", name, e))?;
        module.clear_context(&mut ctx);
    }
    Ok(())
}

/// Collect closures that need pre-compilation as top-level functions.
/// Only collects closures assigned to variables or passed as direct function arguments.
/// Closures used inline in method calls (map/filter) are handled by emit_inline_map_filter.
fn collect_closures(stmts: &[roca::Stmt], out: &mut Vec<(Vec<String>, roca::Expr)>) {
    for stmt in stmts {
        match stmt {
            roca::Stmt::Const { value, .. } | roca::Stmt::Let { value, .. } => {
                // Closure assigned to variable: const double = fn(x) -> x * 2
                if let roca::Expr::Closure { params, body } = value {
                    out.push((params.clone(), *body.clone()));
                }
                // Closure passed as argument to a direct function call (not method call)
                if let roca::Expr::Call { target, args } = value {
                    if matches!(target.as_ref(), roca::Expr::Ident(_)) {
                        for a in args {
                            if let roca::Expr::Closure { params, body } = a {
                                out.push((params.clone(), *body.clone()));
                            }
                        }
                    }
                }
            }
            roca::Stmt::Return(expr) | roca::Stmt::Expr(expr) => {
                if let roca::Expr::Call { target, args } = expr {
                    if matches!(target.as_ref(), roca::Expr::Ident(_)) {
                        for a in args {
                            if let roca::Expr::Closure { params, body } = a {
                                out.push((params.clone(), *body.clone()));
                            }
                        }
                    }
                }
            }
            roca::Stmt::If { then_body, else_body, .. } => {
                collect_closures(then_body, out);
                if let Some(body) = else_body { collect_closures(body, out); }
            }
            roca::Stmt::While { body, .. } | roca::Stmt::For { body, .. } => {
                collect_closures(body, out);
            }
            _ => {}
        }
    }
}

/// Compile a Roca function to native code. Returns the FuncId.
pub fn compile_function<M: Module>(
    module: &mut M,
    func: &roca::FnDef,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    func_return_kinds: &HashMap<String, ValKind>,
    enum_variants: &HashMap<String, Vec<String>>,
    struct_defs: &HashMap<String, Vec<roca::Field>>,
) -> Result<FuncId, String> {
    // Build signature
    let mut sig = module.make_signature();
    for param in &func.params {
        sig.params.push(AbiParam::new(roca_to_cranelift(&param.type_ref)));
    }
    sig.returns.push(AbiParam::new(roca_to_cranelift(&func.return_type)));
    if func.returns_err {
        sig.returns.push(AbiParam::new(types::I8));
    }

    let func_id = module.declare_function(&func.name, Linkage::Export, &sig)
        .map_err(|e| format!("declare: {}", e))?;
    compiled.funcs.insert(func.name.clone(), func_id);

    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    let mut bc = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut bc);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    // Build emit context with runtime + compiled function refs
    let ret_type = roca_to_cranelift(&func.return_type);
    let mut crash_handlers = HashMap::new();
    if let Some(crash) = &func.crash {
        for handler in &crash.handlers {
            crash_handlers.insert(handler.call.clone(), handler.strategy.clone());
        }
    }
    let mut emit_ctx = EmitCtx {
        vars: HashMap::new(),
        func_refs: rt.import_all(module, &mut builder.func, compiled),
        returns_err: func.returns_err,
        return_type: ret_type,
        struct_layouts: HashMap::new(),
        var_struct_type: HashMap::new(),
        crash_handlers,
        func_return_kinds: func_return_kinds.clone(),
        enum_variants: enum_variants.clone(),
        struct_defs: struct_defs.clone(),
        live_heap_vars: Vec::new(),
        loop_heap_base: 0,
        loop_exit: None,
        loop_header: None,
    };

    // Store params in stack slots
    let block_params: Vec<Value> = builder.block_params(entry).to_vec();
    for (i, p) in func.params.iter().enumerate() {
        let cl_type = roca_to_cranelift(&p.type_ref);
        let slot = alloc_slot(&mut builder, block_params[i]);
        emit_ctx.set_var(p.name.clone(), slot, cl_type);
    }

    // Emit body
    let mut returned = false;
    for stmt in &func.body {
        if returned { break; }
        emit_stmt(&mut builder, stmt, &mut emit_ctx, &mut returned);
    }

    if !returned {
        emit_scope_cleanup(&mut builder, &emit_ctx, None);
        let default_val = default_value(&mut builder, &func.return_type);
        if func.returns_err {
            let no_err = builder.ins().iconst(types::I8, 0);
            builder.ins().return_(&[default_val, no_err]);
        } else {
            builder.ins().return_(&[default_val]);
        }
    }

    builder.finalize();

    module.define_function(func_id, &mut ctx)
        .map_err(|e| format!("compile error in {}: {}", func.name, e))?;
    module.clear_context(&mut ctx);
    Ok(func_id)
}

/// Compile a struct method. `self` is the first parameter (struct pointer).
/// Field access via self.field uses struct_get by field index.
pub fn compile_struct_method<M: Module>(
    module: &mut M,
    func: &roca::FnDef,
    struct_name: &str,
    fields: &[roca::Field],
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    func_return_kinds: &HashMap<String, ValKind>,
    enum_variants: &HashMap<String, Vec<String>>,
    struct_defs: &HashMap<String, Vec<roca::Field>>,
) -> Result<FuncId, String> {
    let qualified = format!("{}.{}", struct_name, func.name);

    // Signature: self (I64) + params
    let mut sig = module.make_signature();
    sig.params.push(AbiParam::new(types::I64)); // self
    for param in &func.params {
        sig.params.push(AbiParam::new(roca_to_cranelift(&param.type_ref)));
    }
    sig.returns.push(AbiParam::new(roca_to_cranelift(&func.return_type)));
    if func.returns_err {
        sig.returns.push(AbiParam::new(types::I8));
    }

    let func_id = module.declare_function(&qualified, Linkage::Export, &sig)
        .map_err(|e| format!("declare method {}: {}", qualified, e))?;
    compiled.funcs.insert(qualified.clone(), func_id);

    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    let mut bc = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut bc);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let ret_type = roca_to_cranelift(&func.return_type);
    let mut crash_handlers = HashMap::new();
    if let Some(crash) = &func.crash {
        for handler in &crash.handlers {
            crash_handlers.insert(handler.call.clone(), handler.strategy.clone());
        }
    }
    let mut emit_ctx = EmitCtx {
        vars: HashMap::new(),
        func_refs: rt.import_all(module, &mut builder.func, compiled),
        returns_err: func.returns_err,
        return_type: ret_type,
        struct_layouts: HashMap::new(),
        var_struct_type: HashMap::new(),
        crash_handlers,
        func_return_kinds: func_return_kinds.clone(),
        enum_variants: enum_variants.clone(),
        struct_defs: struct_defs.clone(),
        live_heap_vars: Vec::new(),
        loop_heap_base: 0,
        loop_exit: None,
        loop_header: None,
    };

    // Register struct field layout for self.field access
    let field_info: Vec<(String, ValKind)> = fields.iter().map(|f| {
        (f.name.clone(), type_ref_to_kind(&f.type_ref))
    }).collect();
    emit_ctx.struct_layouts.insert(struct_name.to_string(), StructLayout { fields: field_info });
    emit_ctx.var_struct_type.insert("self".to_string(), struct_name.to_string());

    // Store params: self is block_params[0], then regular params
    let block_params: Vec<Value> = builder.block_params(entry).to_vec();
    let self_slot = alloc_slot(&mut builder, block_params[0]);
    // self is borrowed — store in vars but NOT in live_heap_vars (don't free at scope exit)
    emit_ctx.vars.insert("self".to_string(), VarInfo {
        slot: self_slot, cranelift_type: types::I64, kind: ValKind::Struct, is_heap: false,
    });

    for (i, p) in func.params.iter().enumerate() {
        let cl_type = roca_to_cranelift(&p.type_ref);
        let slot = alloc_slot(&mut builder, block_params[i + 1]);
        emit_ctx.set_var(p.name.clone(), slot, cl_type);
    }

    // Emit body
    let mut returned = false;
    for stmt in &func.body {
        if returned { break; }
        emit_stmt(&mut builder, stmt, &mut emit_ctx, &mut returned);
    }

    if !returned {
        emit_scope_cleanup(&mut builder, &emit_ctx, None);
        let default_val = default_value(&mut builder, &func.return_type);
        if func.returns_err {
            let no_err = builder.ins().iconst(types::I8, 0);
            builder.ins().return_(&[default_val, no_err]);
        } else {
            builder.ins().return_(&[default_val]);
        }
    }

    builder.finalize();
    module.define_function(func_id, &mut ctx)
        .map_err(|e| format!("compile method {}: {}", qualified, e))?;
    module.clear_context(&mut ctx);
    Ok(func_id)
}

/// Compile a mock stub for an extern fn. Returns the mock value on every call.
pub fn compile_mock_stub<M: Module>(
    module: &mut M,
    extern_fn: &roca::ExternFnDef,
    mock: &roca::MockDef,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
) -> Result<FuncId, String> {
    // Use the first mock entry's value as the return value
    let mock_entry = match mock.entries.first() {
        Some(e) => e,
        None => return Err(format!("empty mock for {}", extern_fn.name)),
    };

    let mut sig = module.make_signature();
    for param in &extern_fn.params {
        sig.params.push(AbiParam::new(roca_to_cranelift(&param.type_ref)));
    }
    sig.returns.push(AbiParam::new(roca_to_cranelift(&extern_fn.return_type)));
    if extern_fn.returns_err {
        sig.returns.push(AbiParam::new(types::I8));
    }

    let func_id = module.declare_function(&extern_fn.name, Linkage::Export, &sig)
        .map_err(|e| format!("declare mock {}: {}", extern_fn.name, e))?;
    compiled.funcs.insert(extern_fn.name.clone(), func_id);

    let mut ctx = module.make_context();
    ctx.func.signature = sig;
    let mut bc = FunctionBuilderContext::new();
    let mut builder = FunctionBuilder::new(&mut ctx.func, &mut bc);

    let entry = builder.create_block();
    builder.append_block_params_for_function_params(entry);
    builder.switch_to_block(entry);
    builder.seal_block(entry);

    let mut emit_ctx = EmitCtx {
        vars: HashMap::new(),
        func_refs: rt.import_all(module, &mut builder.func, compiled),
        returns_err: extern_fn.returns_err,
        return_type: roca_to_cranelift(&extern_fn.return_type),
        struct_layouts: HashMap::new(),
        var_struct_type: HashMap::new(),
        crash_handlers: HashMap::new(),
        func_return_kinds: HashMap::new(),
        enum_variants: HashMap::new(),
        struct_defs: HashMap::new(),
        live_heap_vars: Vec::new(),
        loop_heap_base: 0,
        loop_exit: None,
        loop_header: None,
    };

    let val = emit_expr(&mut builder, &mock_entry.value, &mut emit_ctx);
    if extern_fn.returns_err {
        let no_err = builder.ins().iconst(types::I8, 0);
        builder.ins().return_(&[val, no_err]);
    } else {
        builder.ins().return_(&[val]);
    }

    builder.finalize();
    module.define_function(func_id, &mut ctx)
        .map_err(|e| format!("compile mock {}: {}", extern_fn.name, e))?;
    module.clear_context(&mut ctx);
    Ok(func_id)
}

fn default_value(b: &mut FunctionBuilder, ty: &roca::TypeRef) -> Value {
    match ty {
        roca::TypeRef::Number => b.ins().f64const(0.0),
        roca::TypeRef::Bool => b.ins().iconst(types::I8, 0),
        _ => b.ins().iconst(types::I64, 0),
    }
}

fn type_ref_to_kind(ty: &roca::TypeRef) -> ValKind {
    match ty {
        roca::TypeRef::Number => ValKind::Number,
        roca::TypeRef::Bool => ValKind::Bool,
        roca::TypeRef::String => ValKind::String,
        roca::TypeRef::Ok => ValKind::Bool,
        roca::TypeRef::Named(_) => ValKind::Struct,
        roca::TypeRef::Generic(name, _) => match name.as_str() {
            "Array" => ValKind::Array,
            "Map" => ValKind::Struct,
            _ => ValKind::Struct,
        },
        roca::TypeRef::Nullable(_) => ValKind::Other,
        roca::TypeRef::Fn(_, _) => ValKind::Other, // function pointers
    }
}

fn infer_kind(expr: &Expr, ctx: &EmitCtx) -> ValKind {
    match expr {
        Expr::Number(_) => ValKind::Number,
        Expr::Bool(_) => ValKind::Bool,
        Expr::String(_) | Expr::StringInterp(_) => ValKind::String,
        Expr::Array(_) => ValKind::Array,
        Expr::BinOp { op, left, .. } => match op {
            BinOp::Add => {
                // String + String = String, Number + Number = Number
                let left_kind = infer_kind(left, ctx);
                if left_kind == ValKind::String { ValKind::String } else { ValKind::Number }
            }
            BinOp::Sub | BinOp::Mul | BinOp::Div => ValKind::Number,
            _ => ValKind::Other, // comparisons return I64
        },
        Expr::Call { target, .. } => {
            // Direct function call — look up return kind
            if let Expr::Ident(name) = target.as_ref() {
                if let Some(&kind) = ctx.func_return_kinds.get(name) {
                    return kind;
                }
            }
            // Enum data variant call: Token.Number(42)
            if let Expr::FieldAccess { target: obj, field } = target.as_ref() {
                if let Expr::Ident(name) = obj.as_ref() {
                    if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
                        return ValKind::EnumVariant;
                    }
                }
            }
            // Method call — infer from method name
            if let Expr::FieldAccess { field, .. } = target.as_ref() {
                return match field.as_str() {
                    "map" | "filter" | "split" => ValKind::Array,
                    "trim" | "toUpperCase" | "toLowerCase" | "slice"
                    | "charAt" | "join" | "toString" | "concat" => ValKind::String,
                    "indexOf" | "length" | "len" => ValKind::Number,
                    "includes" | "startsWith" | "endsWith" => ValKind::Bool,
                    "push" | "pop" => ValKind::Other,
                    _ => ValKind::Other,
                };
            }
            ValKind::Other
        }
        Expr::StructLit { .. } => ValKind::Struct,
        Expr::EnumVariant { .. } => ValKind::EnumVariant,
        Expr::Match { arms, .. } => {
            // Match returns the kind of its first non-default arm
            for arm in arms {
                if arm.pattern.is_some() {
                    return infer_kind(&arm.value, ctx);
                }
            }
            ValKind::Other
        }
        Expr::Ident(name) => ctx.get_var(name).map(|v| v.kind).unwrap_or(ValKind::Other),
        // Token.Plus (unit variant via field access)
        Expr::FieldAccess { target, field } => {
            if let Expr::Ident(name) = target.as_ref() {
                if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
                    return ValKind::EnumVariant;
                }
            }
            ValKind::Other
        }
        Expr::Null => ValKind::Other,
        _ => ValKind::Other,
    }
}

/// Release all live heap variables except `skip_name` (the return value).
fn emit_scope_cleanup(b: &mut FunctionBuilder, ctx: &EmitCtx, skip_name: Option<&str>) {
    let rc_release = ctx.func_refs.get("__rc_release").copied();
    let free_array = ctx.func_refs.get("__free_array").copied();
    let free_struct = ctx.func_refs.get("__free_struct").copied();

    for var_name in &ctx.live_heap_vars {
        if skip_name == Some(var_name.as_str()) { continue; }
        if let Some(var) = ctx.vars.get(var_name) {
            if !var.is_heap { continue; }
            emit_free_by_kind(b, var.slot, var.cranelift_type, var.kind, rc_release, free_array, free_struct);
        }
    }
}

fn emit_free_by_kind(
    b: &mut FunctionBuilder,
    slot: ir::StackSlot,
    cl_type: ir::Type,
    kind: ValKind,
    rc_release: Option<FuncRef>,
    free_array: Option<FuncRef>,
    free_struct: Option<FuncRef>,
) {
    let ptr = load_slot(b, slot, cl_type);
    match kind {
        ValKind::String => { if let Some(f) = rc_release { call_void(b, f, &[ptr]); } }
        ValKind::Array => { if let Some(f) = free_array { call_void(b, f, &[ptr]); } }
        ValKind::Struct => {
            if let Some(f) = free_struct {
                let zero = b.ins().iconst(types::I64, 0);
                call_void(b, f, &[ptr, zero]);
            }
        }
        ValKind::EnumVariant => {
            // Enum variants have tag string at slot 0 — cascade-release it
            if let Some(f) = free_struct {
                let one = b.ins().iconst(types::I64, 1);
                call_void(b, f, &[ptr, one]);
            }
        }
        // Other/Number/Bool — don't free (either not heap or ambiguous)
        _ => {}
    }
}

/// Release only the loop-body locals (vars declared after loop_heap_base).
fn emit_loop_body_cleanup(b: &mut FunctionBuilder, ctx: &EmitCtx) {
    let rc_release = ctx.func_refs.get("__rc_release").copied();
    let free_array = ctx.func_refs.get("__free_array").copied();
    let free_struct = ctx.func_refs.get("__free_struct").copied();

    for var_name in ctx.live_heap_vars.iter().skip(ctx.loop_heap_base) {
        if let Some(var) = ctx.vars.get(var_name) {
            if !var.is_heap { continue; }
            emit_free_by_kind(b, var.slot, var.cranelift_type, var.kind, rc_release, free_array, free_struct);
        }
    }
}

// ─── Statements ────────────────────────────────────────

fn emit_stmt(b: &mut FunctionBuilder, stmt: &Stmt, ctx: &mut EmitCtx, returned: &mut bool) {
    match stmt {
        Stmt::Const { name, value, .. } | Stmt::Let { name, value, .. } => {
            let kind = infer_kind(value, ctx);
            // Track struct type for field access
            if let Expr::StructLit { name: struct_name, .. } = value {
                ctx.var_struct_type.insert(name.clone(), struct_name.clone());
            }
            let val = emit_expr(b, value, ctx);
            let cl_type = b.func.dfg.value_type(val);
            let slot = alloc_slot(b, val);
            ctx.set_var_kind(name.clone(), slot, cl_type, kind);
        }
        Stmt::Return(expr) => {
            // Identify the return value's variable name (if any) to skip its release
            let skip = if let Expr::Ident(name) = expr { Some(name.as_str()) } else { None };
            let val = emit_expr(b, expr, ctx);
            emit_scope_cleanup(b, ctx, skip);
            if ctx.returns_err {
                let no_err = b.ins().iconst(types::I8, 0);
                b.ins().return_(&[val, no_err]);
            } else {
                b.ins().return_(&[val]);
            }
            *returned = true;
        }
        Stmt::Expr(expr) => { emit_expr(b, expr, ctx); }
        Stmt::If { condition, then_body, else_body, .. } => {
            let cond = emit_expr(b, condition, ctx);
            let then_block = b.create_block();
            let else_block = b.create_block();
            let merge_block = b.create_block();
            b.ins().brif(cond, then_block, &[], else_block, &[]);

            // Save full scope state — branch-local vars must not shadow outer bindings
            let heap_base = ctx.live_heap_vars.len();
            let saved_vars = ctx.vars.clone();
            let saved_struct_types = ctx.var_struct_type.clone();

            b.switch_to_block(then_block);
            b.seal_block(then_block);
            let mut then_ret = false;
            for s in then_body { if then_ret { break; } emit_stmt(b, s, ctx, &mut then_ret); }
            if !then_ret { b.ins().jump(merge_block, &[]); }

            // Restore before else — then-branch vars must not be visible
            ctx.live_heap_vars.truncate(heap_base);
            ctx.vars = saved_vars.clone();
            ctx.var_struct_type = saved_struct_types.clone();

            b.switch_to_block(else_block);
            b.seal_block(else_block);
            let mut else_ret = false;
            if let Some(body) = else_body {
                for s in body { if else_ret { break; } emit_stmt(b, s, ctx, &mut else_ret); }
            }
            if !else_ret { b.ins().jump(merge_block, &[]); }

            // Restore after if/else — branch-local vars not live after merge
            ctx.live_heap_vars.truncate(heap_base);
            ctx.vars = saved_vars;
            ctx.var_struct_type = saved_struct_types;

            b.switch_to_block(merge_block);
            b.seal_block(merge_block);
        }
        Stmt::While { condition, body, .. } => {
            let header = b.create_block();
            let body_block = b.create_block();
            let exit = b.create_block();

            let prev_exit = ctx.loop_exit.replace(exit);
            let prev_header = ctx.loop_header.replace(header);
            let prev_heap_base = ctx.loop_heap_base;
            ctx.loop_heap_base = ctx.live_heap_vars.len();

            b.ins().jump(header, &[]);
            b.switch_to_block(header);
            let cond = emit_expr(b, condition, ctx);
            b.ins().brif(cond, body_block, &[], exit, &[]);

            b.switch_to_block(body_block);
            b.seal_block(body_block);
            let mut body_ret = false;
            for s in body { if body_ret { break; } emit_stmt(b, s, ctx, &mut body_ret); }
            if !body_ret {
                emit_loop_body_cleanup(b, ctx);
                b.ins().jump(header, &[]);
            }
            b.seal_block(header);

            b.switch_to_block(exit);
            b.seal_block(exit);

            // Remove loop-body vars from live_heap_vars — they were freed by loop cleanup
            ctx.live_heap_vars.truncate(ctx.loop_heap_base);
            ctx.loop_heap_base = prev_heap_base;
            ctx.loop_exit = prev_exit;
            ctx.loop_header = prev_header;
        }
        Stmt::For { binding, iter, body } => {
            // For loops over arrays: get length, iterate with index counter
            let arr = emit_expr(b, iter, ctx);
            let len_ref = ctx.get_func("__array_len").copied();

            let len = if let Some(f) = len_ref {
                call_rt(b, f, &[arr])
            } else {
                if b.func.dfg.value_type(arr) == types::F64 {
                    b.ins().fcvt_to_sint(types::I64, arr)
                } else {
                    arr
                }
            };

            // Store arr and len in slots so they survive across blocks
            let arr_slot = alloc_slot(b, arr);
            let len_slot = alloc_slot(b, len);

            // Index counter
            let zero_i64 = b.ins().iconst(types::I64, 0);
            let idx_slot = alloc_slot(b, zero_i64);

            let header = b.create_block();
            let body_block = b.create_block();
            let exit = b.create_block();

            let prev_exit = ctx.loop_exit.replace(exit);
            let prev_header = ctx.loop_header.replace(header);
            let prev_heap_base = ctx.loop_heap_base;
            ctx.loop_heap_base = ctx.live_heap_vars.len();

            b.ins().jump(header, &[]);
            b.switch_to_block(header);

            let idx = load_slot(b, idx_slot, types::I64);
            let len_val = load_slot(b, len_slot, types::I64);
            let cond = b.ins().icmp(ir::condcodes::IntCC::SignedLessThan, idx, len_val);
            b.ins().brif(cond, body_block, &[], exit, &[]);

            b.switch_to_block(body_block);
            b.seal_block(body_block);

            let idx_val = load_slot(b, idx_slot, types::I64);
            let cur_arr = load_slot(b, arr_slot, types::I64);
            if let Some(f) = ctx.get_func("__array_get_f64") {
                let elem = call_rt(b, *f, &[cur_arr, idx_val]);
                let elem_slot = alloc_slot(b, elem);
                ctx.set_var_kind(binding.clone(), elem_slot, types::F64, ValKind::Number);
            } else {
                let idx_f = b.ins().fcvt_from_sint(types::F64, idx_val);
                let elem_slot = alloc_slot(b, idx_f);
                ctx.set_var(binding.clone(), elem_slot, types::F64);
            }

            let mut body_ret = false;
            for s in body { if body_ret { break; } emit_stmt(b, s, ctx, &mut body_ret); }

            if !body_ret {
                emit_loop_body_cleanup(b, ctx);
                let cur = load_slot(b, idx_slot, types::I64);
                let one = b.ins().iconst(types::I64, 1);
                let next = b.ins().iadd(cur, one);
                b.ins().stack_store(next, idx_slot, 0);
                b.ins().jump(header, &[]);
            }
            b.seal_block(header);

            b.switch_to_block(exit);
            b.seal_block(exit);

            ctx.live_heap_vars.truncate(ctx.loop_heap_base);
            ctx.loop_heap_base = prev_heap_base;
            ctx.loop_exit = prev_exit;
            ctx.loop_header = prev_header;
        }
        Stmt::Break => {
            if let Some(exit) = ctx.loop_exit {
                emit_loop_body_cleanup(b, ctx);
                b.ins().jump(exit, &[]);
                *returned = true;
            }
        }
        Stmt::Continue => {
            if let Some(header) = ctx.loop_header {
                emit_loop_body_cleanup(b, ctx);
                b.ins().jump(header, &[]);
                *returned = true; // Block is terminated
            }
        }
        Stmt::Assign { name, value } => {
            if let Some(var) = ctx.get_var(name) {
                let slot = var.slot;
                let is_heap = var.is_heap;
                let cl_type = var.cranelift_type;
                let kind = var.kind;
                if is_heap {
                    let rc_release = ctx.func_refs.get("__rc_release").copied();
                    let free_array = ctx.func_refs.get("__free_array").copied();
                    let free_struct = ctx.func_refs.get("__free_struct").copied();
                    emit_free_by_kind(b, slot, cl_type, kind, rc_release, free_array, free_struct);
                }
                let val = emit_expr(b, value, ctx);
                b.ins().stack_store(val, slot, 0);
            }
        }
        Stmt::FieldAssign { target, field, value } => {
            let var_name = match target {
                Expr::Ident(name) => Some(name.as_str()),
                Expr::SelfRef => Some("self"),
                _ => None,
            };
            if let Some(var_name) = var_name {
                if let Some(struct_name) = ctx.var_struct_type.get(var_name).cloned() {
                    if let Some(layout) = ctx.struct_layouts.get(&struct_name) {
                        if let Some(idx) = layout.field_index(field) {
                            let obj = if let Some(var) = ctx.get_var(var_name) {
                                load_slot(b, var.slot, var.cranelift_type)
                            } else { return; };
                            let val = emit_expr(b, value, ctx);
                            let idx_val = b.ins().iconst(types::I64, idx as i64);
                            emit_struct_set(b, obj, idx_val, val, ctx);
                        }
                    }
                }
            }
        }
        Stmt::LetResult { name, err_name, value } => {
            // Call a function that returns (value, err_tag)
            if let Expr::Call { target, args } = value {
                if let Expr::Ident(fn_name) = target.as_ref() {
                    if let Some(func_ref) = ctx.get_func(fn_name) {
                        let func_ref = *func_ref;
                        let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(b, a, ctx)).collect();
                        let call = b.ins().call(func_ref, &arg_vals);
                        let results = b.inst_results(call).to_vec();
                        if results.len() >= 2 {
                            // Multi-return: (value, err_tag)
                            let val = results[0];
                            let err = results[1];
                            let cl_type = b.func.dfg.value_type(val);
                            let val_slot = alloc_slot(b, val);
                            let kind = if cl_type == types::F64 { ValKind::Number } else { ValKind::Other };
                            ctx.set_var_kind(name.clone(), val_slot, cl_type, kind);
                            let err_slot = alloc_slot(b, err);
                            ctx.set_var_kind(err_name.clone(), err_slot, types::I8, ValKind::Bool);
                        } else if !results.is_empty() {
                            // Single return — no error
                            let val = results[0];
                            let cl_type = b.func.dfg.value_type(val);
                            let val_slot = alloc_slot(b, val);
                            ctx.set_var(name.clone(), val_slot, cl_type);
                            let zero = b.ins().iconst(types::I8, 0);
                            let err_slot = alloc_slot(b, zero);
                            ctx.set_var_kind(err_name.clone(), err_slot, types::I8, ValKind::Bool);
                        }
                    }
                }
            }
        }
        Stmt::ReturnErr { name, .. } => {
            if ctx.returns_err {
                emit_scope_cleanup(b, ctx, None);
                let tag = (name.bytes().fold(1u8, |a, c| a.wrapping_add(c))).max(1);
                let default_val = default_for_ir_type(b, ctx.return_type);
                let err_tag = b.ins().iconst(types::I8, tag as i64);
                b.ins().return_(&[default_val, err_tag]);
                *returned = true;
            }
        }
        _ => {}
    }
}

// ─── Emit helpers ─────────────────────────────────────

fn first_arg_or_null(b: &mut FunctionBuilder, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    args.first().map(|a| emit_expr(b, a, ctx))
        .unwrap_or_else(|| b.ins().iconst(types::I64, 0))
}

fn emit_array_push(b: &mut FunctionBuilder, arr: Value, val: Value, ctx: &mut EmitCtx) {
    let ty = b.func.dfg.value_type(val);
    if ty == types::F64 {
        if let Some(&f) = ctx.get_func("__array_push_f64") { call_void(b, f, &[arr, val]); }
    } else {
        if let Some(&f) = ctx.get_func("__array_push_str") { call_void(b, f, &[arr, val]); }
    }
}

fn emit_struct_set(b: &mut FunctionBuilder, ptr: Value, idx: Value, val: Value, ctx: &mut EmitCtx) {
    let ty = b.func.dfg.value_type(val);
    if ty == types::F64 {
        if let Some(&f) = ctx.get_func("__struct_set_f64") { call_void(b, f, &[ptr, idx, val]); }
    } else {
        if let Some(&f) = ctx.get_func("__struct_set_ptr") { call_void(b, f, &[ptr, idx, val]); }
    }
}

fn emit_length(b: &mut FunctionBuilder, obj: Value, kind: ValKind, ctx: &mut EmitCtx) -> Value {
    let len_func = if kind == ValKind::Array {
        ctx.get_func("__array_len").copied()
    } else {
        ctx.get_func("__string_len").copied()
    };
    if let Some(f) = len_func {
        let len = call_rt(b, f, &[obj]);
        b.ins().fcvt_from_sint(types::F64, len)
    } else {
        b.ins().f64const(0.0)
    }
}

// ─── Expressions ───────────────────────────────────────

fn emit_expr(b: &mut FunctionBuilder, expr: &Expr, ctx: &mut EmitCtx) -> Value {
    match expr {
        Expr::Number(n) => b.ins().f64const(*n),
        Expr::Bool(v) => b.ins().iconst(types::I8, if *v { 1 } else { 0 }),
        Expr::String(s) => {
            // Allocate RC'd string from static literal
            let static_ptr = leak_cstr(b, s);
            if let Some(&f) = ctx.get_func("__string_new") {
                call_rt(b, f, &[static_ptr])
            } else {
                static_ptr
            }
        }
        Expr::Ident(name) => {
            if let Some(var) = ctx.get_var(name) {
                load_slot(b, var.slot, var.cranelift_type)
            } else {
                b.ins().iconst(types::I64, 0)
            }
        }
        Expr::BinOp { left, op, right } => {
            // Check if left is a temp string (concat intermediate, not a variable)
            // String literals are RC-allocated (via __string_new) and must be freed after concat.
            // Only Ident (variable reference) is excluded — variables are freed at scope exit.
            let l_is_temp_string = matches!(op, BinOp::Add)
                && !matches!(left.as_ref(), Expr::Ident(_) | Expr::Number(_) | Expr::Bool(_) | Expr::Null)
                && infer_kind(left, ctx) == ValKind::String;
            let l = emit_expr(b, left, ctx);
            let r = emit_expr(b, right, ctx);
            let result = emit_binop(b, op, l, r, ctx);
            // Free the intermediate after concat consumed it
            if l_is_temp_string {
                if let Some(&f) = ctx.get_func("__rc_release") {
                    call_void(b, f, &[l]);
                }
            }
            result
        }
        Expr::StructLit { name, fields } => emit_struct_lit(b, name, fields, ctx),
        Expr::Call { target, args } => emit_call(b, target, args, ctx),
        Expr::Array(elements) => emit_array_literal(b, elements, ctx),
        Expr::Index { target, index } => emit_index(b, target, index, ctx),
        Expr::Not(inner) => {
            let val = emit_expr(b, inner, ctx);
            let zero = b.ins().iconst(types::I64, 0);
            icmp_to_i64(b, ir::condcodes::IntCC::Equal, val, zero)
        }
        Expr::Closure { params, body } => emit_closure(b, params, body, ctx),
        Expr::SelfRef => {
            if let Some(var) = ctx.get_var("self") {
                load_slot(b, var.slot, var.cranelift_type)
            } else {
                b.ins().iconst(types::I64, 0)
            }
        }
        Expr::Null => b.ins().iconst(types::I64, 0),
        Expr::StringInterp(parts) => emit_string_interp(b, parts, ctx),
        Expr::Match { value, arms } => emit_match(b, value, arms, ctx),
        Expr::FieldAccess { target, field } => emit_field_access(b, target, field, ctx),
        Expr::EnumVariant { enum_name: _, variant, args } => {
            emit_enum_variant(b, variant, args, ctx)
        }
        _ => b.ins().iconst(types::I64, 0),
    }
}

fn emit_binop(b: &mut FunctionBuilder, op: &BinOp, l: Value, r: Value, ctx: &mut EmitCtx) -> Value {
    let is_float = b.func.dfg.value_type(l) == types::F64;
    use ir::condcodes::FloatCC;

    match op {
        BinOp::Add if is_float => b.ins().fadd(l, r),
        BinOp::Add => {
            if let Some(f) = ctx.get_func("__string_concat") { call_rt(b, *f, &[l, r]) }
            else { b.ins().iadd(l, r) }
        }
        BinOp::Sub => b.ins().fsub(l, r),
        BinOp::Mul => b.ins().fmul(l, r),
        BinOp::Div => b.ins().fdiv(l, r),
        BinOp::Eq if is_float => fcmp_to_i64(b, FloatCC::Equal, l, r),
        BinOp::Eq => {
            if let Some(f) = ctx.get_func("__string_eq") {
                let result = call_rt(b, *f, &[l, r]);
                b.ins().uextend(types::I64, result)
            } else {
                icmp_to_i64(b, ir::condcodes::IntCC::Equal, l, r)
            }
        }
        BinOp::Neq if is_float => fcmp_to_i64(b, FloatCC::NotEqual, l, r),
        BinOp::Neq => {
            if let Some(f) = ctx.get_func("__string_eq") {
                let eq = call_rt(b, *f, &[l, r]);
                let ext = b.ins().uextend(types::I64, eq);
                let one = b.ins().iconst(types::I64, 1);
                b.ins().isub(one, ext)
            } else {
                icmp_to_i64(b, ir::condcodes::IntCC::NotEqual, l, r)
            }
        }
        BinOp::Lt => fcmp_to_i64(b, FloatCC::LessThan, l, r),
        BinOp::Gt => fcmp_to_i64(b, FloatCC::GreaterThan, l, r),
        BinOp::Lte => fcmp_to_i64(b, FloatCC::LessThanOrEqual, l, r),
        BinOp::Gte => fcmp_to_i64(b, FloatCC::GreaterThanOrEqual, l, r),
        BinOp::And => bool_and(b, l, r),
        BinOp::Or => bool_or(b, l, r),
    }
}

fn emit_string_interp(b: &mut FunctionBuilder, parts: &[StringPart], ctx: &mut EmitCtx) -> Value {
    let concat = ctx.get_func("__string_concat").copied();
    let to_str = ctx.get_func("__string_from_f64").copied();

    let string_new = ctx.get_func("__string_new").copied();

    let mut result: Option<Value> = None;
    for part in parts {
        let val = match part {
            StringPart::Literal(s) => {
                let static_ptr = leak_cstr(b, s);
                if let Some(f) = string_new { call_rt(b, f, &[static_ptr]) } else { static_ptr }
            }
            StringPart::Expr(expr) => {
                let v = emit_expr(b, expr, ctx);
                // Convert numbers to string for interpolation
                if b.func.dfg.value_type(v) == types::F64 {
                    if let Some(f) = to_str { call_rt(b, f, &[v]) } else { v }
                } else {
                    v
                }
            }
        };
        result = Some(match result {
            None => val,
            Some(acc) => {
                if let Some(f) = concat { call_rt(b, f, &[acc, val]) } else { val }
            }
        });
    }
    result.unwrap_or_else(|| b.ins().iconst(types::I64, 0))
}

fn emit_match(b: &mut FunctionBuilder, value: &Expr, arms: &[roca::MatchArm], ctx: &mut EmitCtx) -> Value {
    let scrutinee = emit_expr(b, value, ctx);
    let is_float = b.func.dfg.value_type(scrutinee) == types::F64;
    let has_variant_patterns = arms.iter().any(|a| matches!(&a.pattern, Some(roca::MatchPattern::Variant { .. })));

    // Result type: infer from the default/wildcard arm value.
    // The default arm is always a concrete expression (no unbound bindings).
    // This correctly handles:
    //   match code { 1 => Shape.Circle(5), _ => Shape.Empty }  → I64 (struct)
    //   match shape { Shape.Circle(r) => r * r, _ => 0 }       → F64 (number)
    //   match n { 1 => 100, 2 => 200, _ => 0 }                 → F64 (number)
    let default_arm = arms.iter().find(|a| a.pattern.is_none());
    let result_type = if let Some(arm) = default_arm {
        let kind = infer_kind(&arm.value, ctx);
        if kind == ValKind::Number { types::F64 } else { types::I64 }
    } else if let Some(first) = arms.first() {
        // No default arm — infer from the first arm's value expression.
        // This handles variant matches where scrutinee is I64 (enum ptr)
        // but arm results are F64 (numbers).
        let kind = infer_kind(&first.value, ctx);
        if kind == ValKind::Number { types::F64 } else { types::I64 }
    } else if is_float {
        types::F64
    } else {
        types::I64
    };

    let merge = b.create_block();
    b.append_block_param(merge, result_type);

    let mut remaining_arms: Vec<_> = arms.iter().collect();
    let default_arm = remaining_arms.iter().position(|a| a.pattern.is_none());
    let default = default_arm.map(|i| remaining_arms.remove(i));

    // Store scrutinee in a slot so it survives across blocks
    let scrutinee_slot = alloc_slot(b, scrutinee);

    for arm in &remaining_arms {
        match &arm.pattern {
            Some(roca::MatchPattern::Value(pattern)) => {
                let scr = load_slot(b, scrutinee_slot, if is_float { types::F64 } else { types::I64 });
                let pat_val = emit_expr(b, pattern, ctx);
                let cond = if is_float {
                    let cmp = b.ins().fcmp(ir::condcodes::FloatCC::Equal, scr, pat_val);
                    b.ins().uextend(types::I64, cmp)
                } else if let Some(f) = ctx.get_func("__string_eq") {
                    let eq = call_rt(b, *f, &[scr, pat_val]);
                    b.ins().uextend(types::I64, eq)
                } else {
                    icmp_to_i64(b, ir::condcodes::IntCC::Equal, scr, pat_val)
                };

                let then_block = b.create_block();
                let next_block = b.create_block();
                b.ins().brif(cond, then_block, &[], next_block, &[]);

                b.switch_to_block(then_block);
                b.seal_block(then_block);
                let result = emit_expr(b, &arm.value, ctx);
                b.ins().jump(merge, &[BlockArg::Value(result)]);

                b.switch_to_block(next_block);
                b.seal_block(next_block);
            }
            Some(roca::MatchPattern::Variant { variant, bindings, .. }) => {
                // Load scrutinee (enum variant struct pointer)
                let scr = load_slot(b, scrutinee_slot, types::I64);

                // Compare tag (slot 0) with variant name
                let zero_idx = b.ins().iconst(types::I64, 0);
                let tag_ptr = if let Some(&f) = ctx.get_func("__struct_get_ptr") {
                    call_rt(b, f, &[scr, zero_idx])
                } else { b.ins().iconst(types::I64, 0) };

                // Compare tag directly with static C string — roca_string_eq
                // reads raw CStr pointers, so no RC allocation needed.
                let variant_cstr = leak_cstr(b, variant);
                let cond = if let Some(&f) = ctx.get_func("__string_eq") {
                    let eq = call_rt(b, f, &[tag_ptr, variant_cstr]);
                    b.ins().uextend(types::I64, eq)
                } else { b.ins().iconst(types::I64, 0) };

                let then_block = b.create_block();
                let next_block = b.create_block();
                b.ins().brif(cond, then_block, &[], next_block, &[]);

                b.switch_to_block(then_block);
                b.seal_block(then_block);

                // Bind destructured fields: bindings[i] = struct_get(scrutinee, i+1)
                // TODO: all bindings are assumed f64/Number. Variants with String
                // data fields will read garbage (f64 bits of a pointer). Fix requires
                // carrying variant field types from the AST through to emission.
                let scr2 = load_slot(b, scrutinee_slot, types::I64);
                for (i, binding) in bindings.iter().enumerate() {
                    let field_idx = b.ins().iconst(types::I64, (i + 1) as i64);
                    let val = if let Some(&f) = ctx.get_func("__struct_get_f64") {
                        call_rt(b, f, &[scr2, field_idx])
                    } else { b.ins().f64const(0.0) };
                    let slot = alloc_slot(b, val);
                    ctx.set_var_kind(binding.clone(), slot, types::F64, ValKind::Number);
                }

                let result = emit_expr(b, &arm.value, ctx);
                b.ins().jump(merge, &[BlockArg::Value(result)]);

                b.switch_to_block(next_block);
                b.seal_block(next_block);
            }
            None => {} // default handled below
        }
    }

    // Default arm or zero
    let default_val = if let Some(arm) = default {
        emit_expr(b, &arm.value, ctx)
    } else if is_float {
        b.ins().f64const(0.0)
    } else {
        b.ins().iconst(types::I64, 0)
    };
    b.ins().jump(merge, &[BlockArg::Value(default_val)]);

    b.switch_to_block(merge);
    b.seal_block(merge);
    b.block_params(merge)[0]
}

fn target_kind(expr: &Expr, ctx: &mut EmitCtx) -> ValKind {
    match expr {
        Expr::Ident(name) => ctx.get_var(name).map(|v| v.kind).unwrap_or(ValKind::Other),
        Expr::String(_) | Expr::StringInterp(_) => ValKind::String,
        Expr::Array(_) => ValKind::Array,
        Expr::StructLit { .. } => ValKind::Struct,
        Expr::Number(_) => ValKind::Number,
        _ => ValKind::Other,
    }
}

fn emit_field_access(b: &mut FunctionBuilder, target: &Expr, field: &str, ctx: &mut EmitCtx) -> Value {
    let kind = target_kind(target, ctx);

    // Check if target is an enum name — Token.Plus constructs a unit variant
    if let Expr::Ident(name) = target {
        if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&field.to_string())) {
            return emit_enum_variant(b, field, &[], ctx);
        }
    }

    // Check if target is a struct variable (including self)
    let var_name = match target {
        Expr::Ident(name) => Some(name.as_str()),
        Expr::SelfRef => Some("self"),
        _ => None,
    };
    if let Some(var_name) = var_name {
        if let Some(struct_name) = ctx.var_struct_type.get(var_name).cloned() {
            if let Some(layout) = ctx.struct_layouts.get(&struct_name) {
                if let Some(idx) = layout.field_index(field) {
                    let field_kind = layout.field_kind(field);
                    let obj = emit_expr(b, target, ctx);
                    let idx_val = b.ins().iconst(types::I64, idx as i64);
                    return match field_kind {
                        ValKind::Number => {
                            if let Some(f) = ctx.get_func("__struct_get_f64") { call_rt(b, *f, &[obj, idx_val]) }
                            else { b.ins().f64const(0.0) }
                        }
                        _ => {
                            // String, Array, Struct, EnumVariant, Other — all are I64 pointers
                            if let Some(f) = ctx.get_func("__struct_get_ptr") { call_rt(b, *f, &[obj, idx_val]) }
                            else { b.ins().iconst(types::I64, 0) }
                        }
                    };
                }
            }
        }
    }

    let obj = emit_expr(b, target, ctx);
    match field {
        "length" | "len" => emit_length(b, obj, kind, ctx),
        _ => obj,
    }
}

fn emit_array_literal(b: &mut FunctionBuilder, elements: &[Expr], ctx: &mut EmitCtx) -> Value {
    let arr = if let Some(f) = ctx.get_func("__array_new") {
        call_rt(b, *f, &[])
    } else {
        return b.ins().iconst(types::I64, 0);
    };

    for elem in elements {
        let val = emit_expr(b, elem, ctx);
        emit_array_push(b, arr, val, ctx);
    }
    arr
}

fn emit_index(b: &mut FunctionBuilder, target: &Expr, index: &Expr, ctx: &mut EmitCtx) -> Value {
    let arr = emit_expr(b, target, ctx);
    let idx = emit_expr(b, index, ctx);
    let idx_i64 = ensure_i64(b, idx);
    // Default to f64 array access
    if let Some(f) = ctx.get_func("__array_get_f64") {
        call_rt(b, *f, &[arr, idx_i64])
    } else {
        b.ins().f64const(0.0)
    }
}

fn emit_call(b: &mut FunctionBuilder, target: &Expr, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    // Method calls: target.method(args)
    if let Expr::FieldAccess { target: obj, field } = target {
        return emit_method_call(b, obj, field, args, ctx);
    }

    if let Expr::Ident(name) = target {
        if name == "log" {
            if let Some(arg) = args.first() {
                let val = emit_expr(b, arg, ctx);
                let ty = b.func.dfg.value_type(val);
                if ty == types::F64 {
                    if let Some(&f) = ctx.get_func("__print_f64") { call_void(b, f, &[val]); }
                } else if ty == types::I8 {
                    if let Some(&f) = ctx.get_func("__print_bool") { call_void(b, f, &[val]); }
                } else {
                    if let Some(&f) = ctx.get_func("__print") { call_void(b, f, &[val]); }
                }
            }
            return b.ins().iconst(types::I8, 0);
        }
        if let Some(&func_ref) = ctx.get_func(name) {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(b, a, ctx)).collect();
            let call = b.ins().call(func_ref, &arg_vals);
            let results = b.inst_results(call).to_vec();

            if results.len() >= 2 {
                let value = results[0];
                let err_tag = results[1];
                if let Some(handler) = ctx.crash_handlers.get(name).cloned() {
                    return emit_crash_handler(b, value, err_tag, &handler, ctx);
                }
                return value;
            }
            if !results.is_empty() { return results[0]; }
        }

        // Indirect call: variable holds a function pointer (closure)
        if let Some(var) = ctx.get_var(name) {
            if var.cranelift_type == types::I64 {
                let func_ptr = load_slot(b, var.slot, types::I64);
                let mut sig = b.func.signature.clone();
                sig.params.clear();
                sig.returns.clear();
                for _ in args {
                    sig.params.push(AbiParam::new(types::F64));
                }
                sig.returns.push(AbiParam::new(types::F64));
                let sig_ref = b.import_signature(sig);
                let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(b, a, ctx)).collect();
                let call = b.ins().call_indirect(sig_ref, func_ptr, &arg_vals);
                let results = b.inst_results(call);
                if !results.is_empty() { return results[0]; }
            }
        }
    }
    b.ins().iconst(types::I64, 0)
}

// ─── Crash handlers ────────────────────────────────────

fn emit_crash_handler(
    b: &mut FunctionBuilder,
    value: Value,
    err_tag: Value,
    handler: &CrashHandlerKind,
    ctx: &mut EmitCtx,
) -> Value {
    let ok_block = b.create_block();
    let err_block = b.create_block();
    let merge = b.create_block();
    let result_type = b.func.dfg.value_type(value);
    b.append_block_param(merge, result_type);

    // Branch: err_tag == 0 → ok, else → error
    b.ins().brif(err_tag, err_block, &[], ok_block, &[]);

    // OK path: use the value
    b.switch_to_block(ok_block);
    b.seal_block(ok_block);
    b.ins().jump(merge, &[BlockArg::Value(value)]);

    // Error path: apply crash strategy
    b.switch_to_block(err_block);
    b.seal_block(err_block);

    let chain = match handler {
        CrashHandlerKind::Simple(chain) => chain.clone(),
        CrashHandlerKind::Detailed { default, .. } => {
            default.clone().unwrap_or_else(|| vec![CrashStep::Halt])
        }
    };

    let terminates = chain.iter().any(|s| matches!(s, CrashStep::Halt | CrashStep::Panic));
    let err_result = emit_crash_chain(b, &chain, result_type, ctx);

    if !terminates {
        b.ins().jump(merge, &[BlockArg::Value(err_result)]);
    }

    b.switch_to_block(merge);
    b.seal_block(merge);
    b.block_params(merge)[0]
}

fn emit_crash_chain(
    b: &mut FunctionBuilder,
    chain: &[CrashStep],
    result_type: ir::Type,
    ctx: &mut EmitCtx,
) -> Value {
    let mut last_value = default_for_ir_type(b, result_type);

    for step in chain {
        match step {
            CrashStep::Log => {
                // Log the error — for now just print "error"
                let msg_val = leak_cstr(b, "error");
                if let Some(&f) = ctx.get_func("__print") {
                    call_void(b, f, &[msg_val]);
                }
            }
            CrashStep::Halt => {
                emit_scope_cleanup(b, ctx, None);
                if ctx.returns_err {
                    let err = b.ins().iconst(types::I8, 1);
                    b.ins().return_(&[last_value, err]);
                } else {
                    b.ins().return_(&[last_value]);
                }
                return last_value;
            }
            CrashStep::Panic => {
                // Trap — crash the program
                b.ins().trap(ir::TrapCode::unwrap_user(1));
                return last_value;
            }
            CrashStep::Skip => {
                // Swallow error, use default value
                // last_value is already the default
            }
            CrashStep::Fallback(expr) => {
                last_value = emit_expr(b, expr, ctx);
            }
            CrashStep::Retry { attempts, delay_ms: _ } => {
                // For native: simple retry loop (no async delay)
                // Re-emit isn't possible without the call args, so for now
                // retry is a no-op in native and falls through to next step
                let _ = attempts;
            }
        }
    }
    last_value
}

fn emit_struct_lit(b: &mut FunctionBuilder, name: &str, fields: &[(String, Expr)], ctx: &mut EmitCtx) -> Value {
    // Register layout if not already known
    if !ctx.struct_layouts.contains_key(name) {
        ctx.struct_layouts.insert(name.to_string(), StructLayout {
            fields: fields.iter().map(|(n, v)| (n.clone(), infer_kind(v, ctx))).collect(),
        });
    }

    let num_fields = b.ins().iconst(types::I64, fields.len() as i64);
    let ptr = if let Some(f) = ctx.get_func("__struct_alloc") {
        call_rt(b, *f, &[num_fields])
    } else {
        return b.ins().iconst(types::I64, 0);
    };

    // Pre-compute field indices to avoid borrow conflict
    let indices: Vec<usize> = {
        let layout = ctx.struct_layouts.get(name).unwrap();
        fields.iter().map(|(n, _)| layout.field_index(n).unwrap_or(0)).collect()
    };

    for (i, (_, field_expr)) in fields.iter().enumerate() {
        let val = emit_expr(b, field_expr, ctx);
        let idx_val = b.ins().iconst(types::I64, indices[i] as i64);
        emit_struct_set(b, ptr, idx_val, val, ctx);
    }

    // Emit constraint validation guards if struct has constraints
    if let Some(field_defs) = ctx.struct_defs.get(name).cloned() {
        let layout = ctx.struct_layouts.get(name).cloned();
        for field_def in &field_defs {
            if field_def.constraints.is_empty() { continue; }
            // Field must be present in the literal AND use the layout index (not literal index)
            // to read back the stored value correctly even when fields are reordered.
            let layout_idx = layout.as_ref().and_then(|l| l.field_index(&field_def.name));
            if fields.iter().any(|(n, _)| n == &field_def.name) && layout_idx.is_some() {
                let is_string = matches!(field_def.type_ref, roca::TypeRef::String);
                let field_idx = b.ins().iconst(types::I64, layout_idx.unwrap() as i64);

                for constraint in &field_def.constraints {
                    match constraint {
                        roca::Constraint::Min(n) if !is_string => {
                            // Number min: if field < n → trap
                            if let Some(&get) = ctx.get_func("__struct_get_f64") {
                                let val = call_rt(b, get, &[ptr, field_idx]);
                                let min_val = b.ins().f64const(*n);
                                let cmp = b.ins().fcmp(ir::condcodes::FloatCC::LessThan, val, min_val);
                                let cmp_ext = b.ins().uextend(types::I64, cmp);
                                emit_constraint_trap(b, cmp_ext, &field_def.name, &format!("must be >= {}", n), ctx);
                            }
                        }
                        roca::Constraint::Max(n) if !is_string => {
                            if let Some(&get) = ctx.get_func("__struct_get_f64") {
                                let val = call_rt(b, get, &[ptr, field_idx]);
                                let max_val = b.ins().f64const(*n);
                                let cmp = b.ins().fcmp(ir::condcodes::FloatCC::GreaterThan, val, max_val);
                                let cmp_ext = b.ins().uextend(types::I64, cmp);
                                emit_constraint_trap(b, cmp_ext, &field_def.name, &format!("must be <= {}", n), ctx);
                            }
                        }
                        roca::Constraint::Min(n) | roca::Constraint::MinLen(n) if is_string => {
                            if let Some(&get) = ctx.get_func("__struct_get_ptr") {
                                let val = call_rt(b, get, &[ptr, field_idx]);
                                if let Some(&len_fn) = ctx.get_func("__string_len") {
                                    let len = call_rt(b, len_fn, &[val]);
                                    let min_val = b.ins().iconst(types::I64, *n as i64);
                                    let cmp = b.ins().icmp(ir::condcodes::IntCC::SignedLessThan, len, min_val);
                                    let cmp_ext = b.ins().uextend(types::I64, cmp);
                                    emit_constraint_trap(b, cmp_ext, &field_def.name, &format!("min length {}", n), ctx);
                                }
                            }
                        }
                        roca::Constraint::Max(n) | roca::Constraint::MaxLen(n) if is_string => {
                            if let Some(&get) = ctx.get_func("__struct_get_ptr") {
                                let val = call_rt(b, get, &[ptr, field_idx]);
                                if let Some(&len_fn) = ctx.get_func("__string_len") {
                                    let len = call_rt(b, len_fn, &[val]);
                                    let max_val = b.ins().iconst(types::I64, *n as i64);
                                    let cmp = b.ins().icmp(ir::condcodes::IntCC::SignedGreaterThan, len, max_val);
                                    let cmp_ext = b.ins().uextend(types::I64, cmp);
                                    emit_constraint_trap(b, cmp_ext, &field_def.name, &format!("max length {}", n), ctx);
                                }
                            }
                        }
                        roca::Constraint::Contains(s) => {
                            if let Some(&get) = ctx.get_func("__struct_get_ptr") {
                                let val = call_rt(b, get, &[ptr, field_idx]);
                                let needle = leak_cstr(b, s);
                                if let Some(&includes) = ctx.get_func("__string_includes") {
                                    let result = call_rt(b, includes, &[val, needle]);
                                    let not_result = {
                                        let ext = b.ins().uextend(types::I64, result);
                                        let one = b.ins().iconst(types::I64, 1);
                                        b.ins().isub(one, ext)
                                    };
                                    emit_constraint_trap(b, not_result, &field_def.name, &format!("must contain \"{}\"", s), ctx);
                                }
                            }
                        }
                        _ => {} // Default, Pattern (needs regex), non-matching guards
                    }
                }
            }
        }
    }

    ptr
}

/// Emit a constraint violation trap: if cond is non-zero, print error and trap.
fn emit_constraint_trap(b: &mut FunctionBuilder, cond: Value, field: &str, msg: &str, ctx: &EmitCtx) {
    let trap_block = b.create_block();
    let ok_block = b.create_block();
    b.ins().brif(cond, trap_block, &[], ok_block, &[]);

    b.switch_to_block(trap_block);
    b.seal_block(trap_block);
    let err_msg = leak_cstr(b, &format!("{}: {}", field, msg));
    if let Some(&panic_fn) = ctx.get_func("__constraint_panic") {
        call_void(b, panic_fn, &[err_msg]);
    }
    // Return default value after constraint violation (flag is set)
    let default = default_for_ir_type(b, ctx.return_type);
    if ctx.returns_err {
        let err_tag = b.ins().iconst(types::I8, 1);
        b.ins().return_(&[default, err_tag]);
    } else {
        b.ins().return_(&[default]);
    }

    b.switch_to_block(ok_block);
    b.seal_block(ok_block);
}

/// Construct an enum variant as a tagged struct.
/// Layout: [tag_string_ptr, field_0, field_1, ...]
/// Unit variants: [tag_ptr] (1 slot)
/// Data variants: [tag_ptr, data_0, data_1, ...] (1+N slots)
fn emit_enum_variant(b: &mut FunctionBuilder, variant: &str, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    let num_slots = 1 + args.len(); // tag + data fields
    let num_slots_val = b.ins().iconst(types::I64, num_slots as i64);

    let ptr = if let Some(&f) = ctx.get_func("__struct_alloc") {
        call_rt(b, f, &[num_slots_val])
    } else {
        return b.ins().iconst(types::I64, 0);
    };

    // Slot 0: tag string
    let tag = leak_cstr(b, variant);
    let tag_str = if let Some(&f) = ctx.get_func("__string_new") {
        call_rt(b, f, &[tag])
    } else { tag };
    let zero = b.ins().iconst(types::I64, 0);
    emit_struct_set(b, ptr, zero, tag_str, ctx);

    // Slots 1..N: data fields
    for (i, arg) in args.iter().enumerate() {
        let val = emit_expr(b, arg, ctx);
        let idx = b.ins().iconst(types::I64, (i + 1) as i64);
        emit_struct_set(b, ptr, idx, val, ctx);
    }
    ptr
}

fn emit_closure(b: &mut FunctionBuilder, params: &[String], body: &Expr, ctx: &mut EmitCtx) -> Value {
    // Pre-compiled closures are registered with __closure_N names.
    // Look up the closure by matching params+body hash to find the right one.
    // If found, return the function pointer via func_addr.
    let closure_name = format!("__closure_{}_{}", params.len(), closure_hash(params, body));
    if let Some(&func_ref) = ctx.get_func(&closure_name) {
        return b.ins().func_addr(types::I64, func_ref);
    }
    // Fallback for closures that weren't pre-compiled (e.g., inline map/filter)
    b.ins().iconst(types::I64, 0)
}

/// Simple hash for identifying closures by their AST structure
fn closure_hash(params: &[String], body: &Expr) -> u64 {
    use std::hash::{Hash, Hasher, DefaultHasher};
    let mut h = DefaultHasher::new();
    for p in params { p.hash(&mut h); }
    format!("{:?}", body).hash(&mut h);
    h.finish()
}

fn emit_method_call(b: &mut FunctionBuilder, target: &Expr, method: &str, args: &[Expr], ctx: &mut EmitCtx) -> Value {
    // Check if this is an enum data variant constructor: Token.Number(42)
    if let Expr::Ident(name) = target {
        if ctx.enum_variants.get(name).map_or(false, |vs| vs.contains(&method.to_string())) {
            return emit_enum_variant(b, method, args, ctx);
        }
    }

    // Check if this is a struct static method call: Counter.current(c)
    // Resolves to compiled function "Counter.current" with args as (self, ...)
    if let Expr::Ident(type_name) = target {
        let qualified = format!("{}.{}", type_name, method);
        if let Some(&func_ref) = ctx.get_func(&qualified) {
            let arg_vals: Vec<Value> = args.iter().map(|a| emit_expr(b, a, ctx)).collect();
            let call = b.ins().call(func_ref, &arg_vals);
            let results = b.inst_results(call);
            if !results.is_empty() { return results[0]; }
        }
    }

    let kind = target_kind(target, ctx);

    // Inline map/filter before evaluating target — they need the closure
    if (method == "map" || method == "filter") && !args.is_empty() {
        if let Expr::Closure { params, body } = &args[0] {
            return emit_inline_map_filter(b, target, method, params, body, ctx);
        }
    }

    // Detect chained method calls that produce intermediate strings.
    // e.g., s.trim().toUpperCase() — trim() creates a temp that must be freed
    // after toUpperCase() consumes it.
    let target_is_temp_string = !matches!(target, Expr::Ident(_) | Expr::String(_))
        && infer_kind(target, ctx) == ValKind::String;

    let obj = emit_expr(b, target, ctx);

    let result = match method {
        "push" => {
            if let Some(arg) = args.first() {
                let val = emit_expr(b, arg, ctx);
                emit_array_push(b, obj, val, ctx);
            }
            b.ins().iconst(types::I8, 0)
        }
        "pop" => {
            // Decrement length — simplified, just returns last element
            if let Some(&get) = ctx.get_func("__array_get_f64") {
                if let Some(&len_fn) = ctx.get_func("__array_len") {
                    let len = call_rt(b, len_fn, &[obj]);
                    let one = b.ins().iconst(types::I64, 1);
                    let last_idx = b.ins().isub(len, one);
                    return call_rt(b, get, &[obj, last_idx]);
                }
            }
            b.ins().f64const(0.0)
        }
        "join" => {
            let sep = if let Some(arg) = args.first() {
                emit_expr(b, arg, ctx)
            } else {
                leak_cstr(b, ",")
            };
            if let Some(&f) = ctx.get_func("__array_join") {
                call_rt(b, f, &[obj, sep])
            } else { b.ins().iconst(types::I64, 0) }
        }

        "includes" | "contains" => {
            let needle = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_includes") {
                let result = call_rt(b, f, &[obj, needle]);
                b.ins().uextend(types::I64, result)
            } else { b.ins().iconst(types::I64, 0) }
        }
        "startsWith" => {
            let prefix = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_starts_with") {
                let result = call_rt(b, f, &[obj, prefix]);
                b.ins().uextend(types::I64, result)
            } else { b.ins().iconst(types::I64, 0) }
        }
        "endsWith" => {
            let suffix = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_ends_with") {
                let result = call_rt(b, f, &[obj, suffix]);
                b.ins().uextend(types::I64, result)
            } else { b.ins().iconst(types::I64, 0) }
        }
        "trim" => {
            if let Some(&f) = ctx.get_func("__string_trim") {
                call_rt(b, f, &[obj])
            } else { obj }
        }
        "toUpperCase" => {
            if let Some(&f) = ctx.get_func("__string_to_upper") {
                call_rt(b, f, &[obj])
            } else { obj }
        }
        "toLowerCase" => {
            if let Some(&f) = ctx.get_func("__string_to_lower") {
                call_rt(b, f, &[obj])
            } else { obj }
        }
        "slice" => {
            let start = args.first().map(|a| emit_expr(b, a, ctx))
                .unwrap_or_else(|| b.ins().iconst(types::I64, 0));
            let end = args.get(1).map(|a| emit_expr(b, a, ctx))
                .unwrap_or_else(|| {
                    if let Some(&f) = ctx.get_func("__string_len") {
                        call_rt(b, f, &[obj])
                    } else { b.ins().iconst(types::I64, 0) }
                });
            let start_i = ensure_i64(b, start);
            let end_i = ensure_i64(b, end);
            if let Some(&f) = ctx.get_func("__string_slice") {
                call_rt(b, f, &[obj, start_i, end_i])
            } else { obj }
        }
        "split" => {
            let delim = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_split") {
                call_rt(b, f, &[obj, delim])
            } else { b.ins().iconst(types::I64, 0) }
        }
        "charAt" => {
            let idx = first_arg_or_null(b, args, ctx);
            let idx_i = ensure_i64(b, idx);
            if let Some(&f) = ctx.get_func("__string_char_at") {
                call_rt(b, f, &[obj, idx_i])
            } else { b.ins().iconst(types::I64, 0) }
        }
        "indexOf" => {
            let needle = first_arg_or_null(b, args, ctx);
            if let Some(&f) = ctx.get_func("__string_index_of") {
                call_rt(b, f, &[obj, needle])
            } else { b.ins().f64const(-1.0) }
        }

        "len" | "length" => emit_length(b, obj, kind, ctx),
        "toString" => {
            let ty = b.func.dfg.value_type(obj);
            if ty == types::F64 {
                if let Some(&f) = ctx.get_func("__string_from_f64") {
                    call_rt(b, f, &[obj])
                } else { b.ins().iconst(types::I64, 0) }
            } else {
                obj
            }
        }
        _ => obj,
    };

    // Free intermediate string from chained method calls (e.g., trim() result
    // after toUpperCase() has consumed it)
    if target_is_temp_string {
        if let Some(&f) = ctx.get_func("__rc_release") {
            call_void(b, f, &[obj]);
        }
    }

    result
}

/// Inline map/filter: emit a loop that applies the closure body to each element.
fn emit_inline_map_filter(
    b: &mut FunctionBuilder,
    target: &Expr,
    method: &str,
    params: &[String],
    body: &Expr,
    ctx: &mut EmitCtx,
) -> Value {
    let arr = emit_expr(b, target, ctx);
    let is_filter = method == "filter";

    // Create result array
    let result_arr = if let Some(&f) = ctx.get_func("__array_new") {
        call_rt(b, f, &[])
    } else { return b.ins().iconst(types::I64, 0); };

    // Get length — store in slot so it's accessible across blocks
    let len = if let Some(&f) = ctx.get_func("__array_len") {
        call_rt(b, f, &[arr])
    } else { return result_arr; };
    let len_slot = alloc_slot(b, len);
    let arr_slot = alloc_slot(b, arr);
    let result_slot = alloc_slot(b, result_arr);

    // Loop: idx = 0; while idx < len
    let zero = b.ins().iconst(types::I64, 0);
    let idx_slot = alloc_slot(b, zero);
    let header = b.create_block();
    let body_block = b.create_block();
    let exit = b.create_block();

    b.ins().jump(header, &[]);
    b.switch_to_block(header);
    let idx = load_slot(b, idx_slot, types::I64);
    let len_val = load_slot(b, len_slot, types::I64);
    let cond = b.ins().icmp(ir::condcodes::IntCC::SignedLessThan, idx, len_val);
    b.ins().brif(cond, body_block, &[], exit, &[]);

    b.switch_to_block(body_block);
    b.seal_block(body_block);

    // Get element
    let cur_idx = load_slot(b, idx_slot, types::I64);
    let cur_arr = load_slot(b, arr_slot, types::I64);
    let elem = if let Some(&f) = ctx.get_func("__array_get_f64") {
        call_rt(b, f, &[cur_arr, cur_idx])
    } else { b.ins().f64const(0.0) };

    // Bind closure param
    let param_name = params.first().cloned().unwrap_or_default();
    let elem_slot = alloc_slot(b, elem);
    ctx.set_var_kind(param_name, elem_slot, types::F64, ValKind::Number);

    // Evaluate closure body
    let result = emit_expr(b, body, ctx);

    if is_filter {
        // If truthy, push original element
        let then_push = b.create_block();
        let after_push = b.create_block();
        b.ins().brif(result, then_push, &[], after_push, &[]);

        b.switch_to_block(then_push);
        b.seal_block(then_push);
        let push_elem = load_slot(b, elem_slot, types::F64);
        let res_arr = load_slot(b, result_slot, types::I64);
        if let Some(&f) = ctx.get_func("__array_push_f64") {
            call_void(b, f, &[res_arr, push_elem]);
        }
        b.ins().jump(after_push, &[]);

        b.switch_to_block(after_push);
        b.seal_block(after_push);
    } else {
        let res_arr = load_slot(b, result_slot, types::I64);
        emit_array_push(b, res_arr, result, ctx);
    }

    // Increment idx
    let next_idx = load_slot(b, idx_slot, types::I64);
    let one = b.ins().iconst(types::I64, 1);
    let incremented = b.ins().iadd(next_idx, one);
    b.ins().stack_store(incremented, idx_slot, 0);
    b.ins().jump(header, &[]);
    b.seal_block(header);

    b.switch_to_block(exit);
    b.seal_block(exit);

    load_slot(b, result_slot, types::I64)
}
