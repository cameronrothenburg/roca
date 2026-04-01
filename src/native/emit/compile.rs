//! Top-level compilation: function declaration, closure pre-compilation, function/method bodies.

use std::collections::HashMap;
use cranelift_codegen::ir::{types, AbiParam, InstBuilder};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{Module, Linkage, FuncId};

use crate::ast::{self as roca};
use crate::native::types::roca_to_cranelift;
use crate::native::runtime::RuntimeFuncs;
use crate::native::helpers::alloc_slot;
use super::context::{CompiledFuncs, EmitCtx, ValKind, StructLayout, VarInfo};
use super::helpers::{type_ref_to_kind, emit_scope_cleanup};
use super::expr::emit_expr;
use super::stmt::emit_stmt;
use super::methods::emit_param_constraints;

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
            roca::Item::ExternContract(c) => {
                for sig in &c.functions {
                    map.insert(format!("{}.{}", c.name, sig.name), type_ref_to_kind(&sig.return_type));
                }
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
        let name = format!("__closure_{}_{}", params.len(), super::expr::closure_hash(&params, &body));
        if compiled.funcs.contains_key(&name) { continue; }

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

        let block_params: Vec<cranelift_codegen::ir::Value> = builder.block_params(entry).to_vec();
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
fn collect_closures(stmts: &[roca::Stmt], out: &mut Vec<(Vec<String>, roca::Expr)>) {
    for stmt in stmts {
        match stmt {
            roca::Stmt::Const { value, .. } | roca::Stmt::Let { value, .. } => {
                if let roca::Expr::Closure { params, body } = value {
                    out.push((params.clone(), *body.clone()));
                }
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

/// Pre-compile wait expressions as zero-arg functions for concurrent execution.
/// Each waitAll/waitFirst sub-expression becomes a standalone JIT function.
pub fn compile_wait_exprs<M: Module>(
    module: &mut M,
    source: &roca::SourceFile,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    func_return_kinds: &HashMap<String, ValKind>,
) -> Result<(), String> {
    let mut wait_exprs = Vec::new();
    for item in &source.items {
        if let roca::Item::Function(f) = item {
            collect_wait_exprs(&f.body, &mut wait_exprs);
        }
    }
    for (name, expr) in wait_exprs {
        if compiled.funcs.contains_key(&name) { continue; }

        // Zero-arg function returning f64
        let mut sig = module.make_signature();
        sig.returns.push(AbiParam::new(types::F64));

        let func_id = module.declare_function(&name, Linkage::Export, &sig)
            .map_err(|e| format!("declare wait expr: {}", e))?;
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

        let val = emit_expr(&mut builder, &expr, &mut emit_ctx);
        emit_scope_cleanup(&mut builder, &emit_ctx, None);
        builder.ins().return_(&[val]);
        builder.finalize();

        module.define_function(func_id, &mut ctx)
            .map_err(|e| format!("compile wait expr {}: {}", name, e))?;
        module.clear_context(&mut ctx);
    }
    Ok(())
}

fn collect_wait_exprs(stmts: &[roca::Stmt], out: &mut Vec<(String, roca::Expr)>) {
    for stmt in stmts {
        match stmt {
            roca::Stmt::Wait { kind: roca::WaitKind::All(exprs), .. }
            | roca::Stmt::Wait { kind: roca::WaitKind::First(exprs), .. } => {
                for expr in exprs {
                    let name = format!("__wait_{}", wait_expr_hash(expr));
                    out.push((name, expr.clone()));
                }
            }
            roca::Stmt::If { then_body, else_body, .. } => {
                collect_wait_exprs(then_body, out);
                if let Some(body) = else_body { collect_wait_exprs(body, out); }
            }
            roca::Stmt::While { body, .. } | roca::Stmt::For { body, .. } => {
                collect_wait_exprs(body, out);
            }
            _ => {}
        }
    }
}

pub(super) fn wait_expr_hash(expr: &roca::Expr) -> u64 {
    expr_debug_hash(expr)
}

/// Shared hash for AST expressions — used by closure and wait compilation.
pub(super) fn expr_debug_hash(expr: &roca::Expr) -> u64 {
    use std::hash::{Hash, Hasher, DefaultHasher};
    let mut h = DefaultHasher::new();
    format!("{:?}", expr).hash(&mut h);
    h.finish()
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

    let block_params: Vec<cranelift_codegen::ir::Value> = builder.block_params(entry).to_vec();
    for (i, p) in func.params.iter().enumerate() {
        let cl_type = roca_to_cranelift(&p.type_ref);
        let slot = alloc_slot(&mut builder, block_params[i]);
        emit_ctx.set_var(p.name.clone(), slot, cl_type);
    }

    // Validate constrained params at function entry
    emit_param_constraints(&mut builder, &func.params, &mut emit_ctx);

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

    let field_info: Vec<(String, ValKind)> = fields.iter().map(|f| {
        (f.name.clone(), type_ref_to_kind(&f.type_ref))
    }).collect();
    emit_ctx.struct_layouts.insert(struct_name.to_string(), StructLayout { fields: field_info });
    emit_ctx.var_struct_type.insert("self".to_string(), struct_name.to_string());

    let block_params: Vec<cranelift_codegen::ir::Value> = builder.block_params(entry).to_vec();
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

    // Validate constrained params at method entry
    emit_param_constraints(&mut builder, &func.params, &mut emit_ctx);

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

/// Compile an auto-stub for an extern fn — returns a default value.
pub fn compile_extern_fn_stub<M: Module>(
    module: &mut M,
    extern_fn: &roca::ExternFnDef,
    default_value: &roca::Expr,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
) -> Result<FuncId, String> {

    let mut sig = module.make_signature();
    for param in &extern_fn.params {
        sig.params.push(AbiParam::new(roca_to_cranelift(&param.type_ref)));
    }
    sig.returns.push(AbiParam::new(roca_to_cranelift(&extern_fn.return_type)));
    if extern_fn.returns_err {
        sig.returns.push(AbiParam::new(types::I8));
    }

    let func_id = module.declare_function(&extern_fn.name, Linkage::Export, &sig)
        .map_err(|e| format!("declare stub {}: {}", extern_fn.name, e))?;
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

    let val = emit_expr(&mut builder, default_value, &mut emit_ctx);
    if extern_fn.returns_err {
        let no_err = builder.ins().iconst(types::I8, 0);
        builder.ins().return_(&[val, no_err]);
    } else {
        builder.ins().return_(&[val]);
    }

    builder.finalize();
    module.define_function(func_id, &mut ctx)
        .map_err(|e| format!("compile stub {}: {}", extern_fn.name, e))?;
    module.clear_context(&mut ctx);
    Ok(func_id)
}

/// Compile auto-stubs for all methods in an extern contract.
/// Each method gets a JIT function named "Contract.method" that returns a default value.
pub fn compile_contract_stubs<M: Module>(
    module: &mut M,
    contract: &roca::ContractDef,
    _rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
) -> Result<(), String> {
    for sig_def in &contract.functions {
        let qualified = format!("{}.{}", contract.name, sig_def.name);
        if compiled.funcs.contains_key(&qualified) { continue; }

        let mut sig = module.make_signature();
        for param in &sig_def.params {
            sig.params.push(AbiParam::new(roca_to_cranelift(&param.type_ref)));
        }
        sig.returns.push(AbiParam::new(roca_to_cranelift(&sig_def.return_type)));
        if sig_def.returns_err {
            sig.returns.push(AbiParam::new(types::I8));
        }

        let func_id = module.declare_function(&qualified, Linkage::Export, &sig)
            .map_err(|e| format!("declare stub {}: {}", qualified, e))?;
        compiled.funcs.insert(qualified.clone(), func_id);

        let mut ctx = module.make_context();
        ctx.func.signature = sig;
        let mut bc = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut bc);

        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        builder.seal_block(entry);

        let ret_val = default_value(&mut builder, &sig_def.return_type);
        if sig_def.returns_err {
            let no_err = builder.ins().iconst(types::I8, 0);
            builder.ins().return_(&[ret_val, no_err]);
        } else {
            builder.ins().return_(&[ret_val]);
        }

        builder.finalize();
        match module.define_function(func_id, &mut ctx) {
            Ok(_) => {}
            Err(_) => {
                // Skip stubs that fail to compile (e.g., generic params)
                module.clear_context(&mut ctx);
                continue;
            }
        }
        module.clear_context(&mut ctx);
    }
    Ok(())
}

pub fn default_value(b: &mut FunctionBuilder, ty: &roca::TypeRef) -> cranelift_codegen::ir::Value {
    match ty {
        roca::TypeRef::Number => b.ins().f64const(0.0),
        roca::TypeRef::Bool => b.ins().iconst(types::I8, 0),
        _ => b.ins().iconst(types::I64, 0),
    }
}
