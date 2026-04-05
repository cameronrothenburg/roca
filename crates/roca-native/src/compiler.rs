//! AST walker that emits Cranelift IR through the builder.

use std::collections::HashMap;

use cranelift_codegen::ir::{
    types, AbiParam, Function, InstBuilder, UserFuncName,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use roca_lang::ast::{BinOp, Expr, ExprKind, FuncDef, Item, Lit, Param, SourceFile, Stmt, Type, UnaryOp};

use crate::builder::{ClifType, RocaBuilder};
use crate::runtime;

// ─── Type mapping ────────────────────────────────────────────────────────────

fn roca_type_to_clif(ty: &Type) -> ClifType {
    match ty {
        Type::Int     => ClifType::I64,
        Type::Float   => ClifType::F64,
        Type::Bool    => ClifType::I8,
        Type::String  => ClifType::I64,
        Type::Named(_)=> ClifType::I64,
        Type::Unit    => ClifType::I64,
        Type::Array(_)=> ClifType::I64,
        Type::Fn(..)  => ClifType::I64,
        Type::Optional(_) => ClifType::I64,
    }
}

// ─── Struct registry ─────────────────────────────────────────────────────────

type StructRegistry = HashMap<String, Vec<(String, Type)>>;

fn build_struct_registry(items: &[Item]) -> StructRegistry {
    let mut map = HashMap::new();
    for item in items {
        if let Item::Struct(s) = item {
            let fields: Vec<(String, Type)> = s.fields.iter()
                .map(|f| (f.name.clone(), f.ty.clone()))
                .collect();
            map.insert(s.name.clone(), fields);
        }
    }
    map
}

fn field_index(registry: &StructRegistry, struct_name: &str, field_name: &str) -> Option<usize> {
    let fields = registry.get(struct_name)?;
    fields.iter().position(|(n, _)| n == field_name)
}

fn field_type(registry: &StructRegistry, struct_name: &str, field_name: &str) -> Option<Type> {
    let fields = registry.get(struct_name)?;
    fields.iter().find(|(n, _)| n == field_name).map(|(_, t)| t.clone())
}

// ─── Function signature registry ─────────────────────────────────────────────

type SigRegistry = HashMap<String, (Vec<ClifType>, ClifType)>;

fn build_sig_registry(items: &[Item]) -> SigRegistry {
    let mut map = HashMap::new();
    for item in items {
        match item {
            Item::Function(f) => {
                let params: Vec<ClifType> = f.params.iter().map(|p| roca_type_to_clif(&p.ty)).collect();
                let ret = roca_type_to_clif(&f.ret);
                map.insert(f.name.clone(), (params, ret));
            }
            Item::Struct(s) => {
                for m in &s.methods {
                    let key = format!("{}.{}", s.name, m.name);
                    let mut params: Vec<ClifType> = Vec::new();
                    if body_uses_self(&m.body) {
                        params.push(ClifType::I64); // self pointer
                    }
                    params.extend(m.params.iter().map(|p| roca_type_to_clif(&p.ty)));
                    let ret = roca_type_to_clif(&m.ret);
                    map.insert(key, (params, ret));
                }
            }
            _ => {}
        }
    }
    map
}

// ─── Compiled module ─────────────────────────────────────────────────────────

pub struct CompiledModule {
    pub jit: JITModule,
    pub func_ids: HashMap<String, FuncId>,
}

// ─── Compile entry point ─────────────────────────────────────────────────────

pub fn compile(source: &SourceFile) -> Result<CompiledModule, String> {
    let isa_builder = cranelift_native::builder()
        .map_err(|e| format!("ISA builder error: {e}"))?;
    let flags = cranelift_codegen::settings::Flags::new(cranelift_codegen::settings::builder());
    let isa = isa_builder.finish(flags).map_err(|e| format!("ISA finish error: {e}"))?;
    let mut builder = cranelift_jit::JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());

    // Register roca-mem symbols so the JIT can resolve them at link time
    runtime::register_symbols(&mut builder);

    let mut module = JITModule::new(builder);
    let runtime_ids = runtime::declare_all(&mut module);
    let struct_reg = build_struct_registry(&source.items);
    let sig_reg = build_sig_registry(&source.items);

    // Declare all user functions
    let mut func_ids: HashMap<String, FuncId> = HashMap::new();
    for item in &source.items {
        match item {
            Item::Function(f) => {
                let sig = make_sig(&module, &f.params, &f.ret);
                let id = module.declare_function(&f.name, Linkage::Export, &sig)
                    .map_err(|e| format!("declare {}: {e}", f.name))?;
                func_ids.insert(f.name.clone(), id);
            }
            Item::Struct(s) => {
                for m in &s.methods {
                    let key = format!("{}.{}", s.name, m.name);
                    let uses_self = body_uses_self(&m.body);
                    let sig = make_sig_with_self(&module, &m.params, &m.ret, uses_self);
                    let id = module.declare_function(&key, Linkage::Export, &sig)
                        .map_err(|e| format!("declare {key}: {e}"))?;
                    func_ids.insert(key, id);
                }
            }
            _ => {}
        }
    }

    // Compile each function
    for item in &source.items {
        match item {
            Item::Function(f) => {
                let id = *func_ids.get(&f.name).ok_or_else(|| format!("function {} not registered", f.name))?;
                compile_function(
                    &mut module, id, f, &f.name,
                    &func_ids, &runtime_ids, &struct_reg, &sig_reg,
                ).map_err(|e| format!("compile {}: {e}", f.name))?;
            }
            Item::Struct(s) => {
                for m in &s.methods {
                    let key = format!("{}.{}", s.name, m.name);
                    let id = *func_ids.get(&key).ok_or_else(|| format!("method {} not registered", key))?;
                    compile_function(
                        &mut module, id, m, &key,
                        &func_ids, &runtime_ids, &struct_reg, &sig_reg,
                    ).map_err(|e| format!("compile {key}: {e}"))?;
                }
            }
            _ => {}
        }
    }

    // Compile call shims: each takes (args_ptr: i64) -> i64 and unpacks args
    // to call the real function. This lets call() work with any number of args.
    let shim_entries: Vec<(String, Vec<ClifType>, ClifType)> = func_ids.keys().map(|name| {
        let (params, ret) = &sig_reg[name];
        (name.clone(), params.clone(), *ret)
    }).collect();

    for (name, param_types, ret_type) in &shim_entries {
        let shim_name = format!("{}__shim", name);
        let mut shim_sig = module.make_signature();
        shim_sig.params.push(AbiParam::new(types::I64)); // args_ptr
        shim_sig.returns.push(AbiParam::new(types::I64)); // unified i64 return
        let shim_id = module.declare_function(&shim_name, Linkage::Export, &shim_sig)
            .map_err(|e| format!("declare shim {shim_name}: {e}"))?;

        compile_shim(
            &mut module, shim_id, &shim_name, &func_ids[name],
            param_types, *ret_type,
        ).map_err(|e| format!("compile shim {shim_name}: {e}"))?;

        func_ids.insert(shim_name, shim_id);
    }

    module.finalize_definitions().map_err(|e| format!("finalize: {e}"))?;

    Ok(CompiledModule { jit: module, func_ids })
}

/// Compile a shim that unpacks args from a pointer and calls the real function.
/// Shim signature: (args_ptr: i64) -> i64
/// Each arg is loaded as 8 bytes from args_ptr[i*8], bitcast to the correct type.
/// The return value is unified to i64 (f64 bits, bool extended, etc.).
fn compile_shim(
    module: &mut JITModule,
    shim_id: FuncId,
    shim_name: &str,
    real_id: &FuncId,
    param_types: &[ClifType],
    ret_type: ClifType,
) -> Result<(), String> {
    let mut cl_ctx = cranelift_codegen::Context::new();
    cl_ctx.func.signature = {
        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(types::I64));
        sig.returns.push(AbiParam::new(types::I64));
        sig
    };

    let mut fb_ctx = FunctionBuilderContext::new();
    let mut fb = FunctionBuilder::new(&mut cl_ctx.func, &mut fb_ctx);

    let entry = fb.create_block();
    fb.append_block_params_for_function_params(entry);
    fb.switch_to_block(entry);
    fb.seal_block(entry);

    let args_ptr = fb.block_params(entry)[0]; // i64 pointer to args buffer

    // Load each arg from the buffer
    let mut call_args = Vec::new();
    for (i, ty) in param_types.iter().enumerate() {
        let offset = i32::try_from(i * 8).map_err(|_| format!("too many params in shim {shim_name}"))?;
        let raw = fb.ins().load(types::I64, cranelift_codegen::ir::MemFlags::trusted(), args_ptr, offset);
        let arg = match ty {
            ClifType::F64 => fb.ins().bitcast(types::F64, cranelift_codegen::ir::MemFlags::new(), raw),
            ClifType::I8 => fb.ins().ireduce(types::I8, raw),
            ClifType::I64 => raw,
        };
        call_args.push(arg);
    }

    // Call the real function
    let real_func_ref = module.declare_func_in_func(*real_id, fb.func);
    let call_inst = fb.ins().call(real_func_ref, &call_args);
    let result = fb.inst_results(call_inst)[0];

    // Unify return to i64
    let unified = match ret_type {
        ClifType::F64 => fb.ins().bitcast(types::I64, cranelift_codegen::ir::MemFlags::new(), result),
        ClifType::I8 => fb.ins().uextend(types::I64, result),
        ClifType::I64 => result,
    };
    fb.ins().return_(&[unified]);
    fb.finalize();

    module.define_function(shim_id, &mut cl_ctx)
        .map_err(|e| format!("define_function {shim_name}: {e}"))?;

    Ok(())
}

fn make_sig(module: &JITModule, params: &[Param], ret: &Type) -> cranelift_codegen::ir::Signature {
    make_sig_with_self(module, params, ret, false)
}

fn make_sig_with_self(module: &JITModule, params: &[Param], ret: &Type, has_self: bool) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
    if has_self {
        sig.params.push(AbiParam::new(types::I64)); // self pointer
    }
    for p in params {
        sig.params.push(AbiParam::new(roca_type_to_clif(&p.ty).to_clif()));
    }
    sig.returns.push(AbiParam::new(roca_type_to_clif(ret).to_clif()));
    sig
}

// ─── Compile context ─────────────────────────────────────────────────────────

struct CompileCtx<'a> {
    module: &'a mut JITModule,
    func_ids: &'a HashMap<String, FuncId>,
    runtime_ids: &'a HashMap<String, FuncId>,
    struct_reg: &'a StructRegistry,
    sig_reg: &'a SigRegistry,
    var_struct_map: HashMap<String, String>,
    closure_counter: usize,
}

impl<'a> CompileCtx<'a> {
    fn import_runtime(&mut self, b: &mut RocaBuilder, name: &str) -> cranelift_codegen::ir::FuncRef {
        let id = self.runtime_ids[name];
        self.module.declare_func_in_func(id, b.builder.func)
    }

}

// ─── Compile one function ─────────────────────────────────────────────────────

/// Check if a function body references `self` anywhere.
fn body_uses_self(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| stmt_uses_self(s))
}

fn stmt_uses_self(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Let { value, .. } | Stmt::Var { value, .. } | Stmt::Assign { value, .. }
        | Stmt::Return(value) | Stmt::Expr(value) => expr_uses_self(value),
        Stmt::SetField { target, value, .. } => expr_uses_self(target) || expr_uses_self(value),
        Stmt::If { cond, then, else_ } => {
            expr_uses_self(cond) || body_uses_self(then)
                || else_.as_ref().is_some_and(|e| body_uses_self(e))
        }
        Stmt::Loop { body } | Stmt::For { body, .. } => body_uses_self(body),
        _ => false,
    }
}

fn expr_uses_self(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::SelfRef => true,
        ExprKind::BinOp { left, right, .. } => expr_uses_self(left) || expr_uses_self(right),
        ExprKind::UnaryOp { expr, .. } => expr_uses_self(expr),
        ExprKind::Call { target, args } => expr_uses_self(target) || args.iter().any(expr_uses_self),
        ExprKind::GetField { target, .. } => expr_uses_self(target),
        ExprKind::StructLit { fields, .. } => fields.iter().any(|(_, v)| expr_uses_self(v)),
        ExprKind::If { cond, then, else_ } => {
            expr_uses_self(cond) || expr_uses_self(then) || else_.as_ref().is_some_and(|e| expr_uses_self(e))
        }
        ExprKind::MakeClosure { body, .. } => expr_uses_self(body),
        _ => false,
    }
}

fn compile_function(
    module: &mut JITModule,
    func_id: FuncId,
    func: &FuncDef,
    func_key: &str,
    func_ids: &HashMap<String, FuncId>,
    runtime_ids: &HashMap<String, FuncId>,
    struct_reg: &StructRegistry,
    sig_reg: &SigRegistry,
) -> Result<(), String> {
    let uses_self = body_uses_self(&func.body);

    let sig = {
        let mut s = module.make_signature();
        // Instance methods get self as first i64 param
        if uses_self {
            s.params.push(AbiParam::new(types::I64));
        }
        for p in &func.params {
            s.params.push(AbiParam::new(roca_type_to_clif(&p.ty).to_clif()));
        }
        s.returns.push(AbiParam::new(roca_type_to_clif(&func.ret).to_clif()));
        s
    };

    let mut cl_func = Function::with_name_signature(
        UserFuncName::user(0, func_id.as_u32()),
        sig,
    );
    let mut fb_ctx = FunctionBuilderContext::new();
    let fb = FunctionBuilder::new(&mut cl_func, &mut fb_ctx);

    let mut ctx = CompileCtx {
        module,
        func_ids,
        runtime_ids,
        struct_reg,
        sig_reg,
        var_struct_map: HashMap::new(),
        closure_counter: 0,
    };

    let mut b = RocaBuilder::new(fb);

    // Create entry block
    let entry = b.create_block();
    b.builder.append_block_params_for_function_params(entry);
    b.builder.switch_to_block(entry);
    b.builder.seal_block(entry);

    // Bind parameters
    let param_offset = if uses_self { 1 } else { 0 };
    if uses_self {
        let self_val = b.builder.block_params(entry)[0];
        b.param_declare("self", ClifType::I64, self_val);
        // Track self's struct type for field access
        if let Some(dot_pos) = func_key.find('.') {
            let struct_name = &func_key[..dot_pos];
            ctx.var_struct_map.insert("self".to_string(), struct_name.to_string());
        }
    }
    let param_vals: Vec<_> = (0..func.params.len())
        .map(|i| b.builder.block_params(entry)[i + param_offset])
        .collect();
    for (i, p) in func.params.iter().enumerate() {
        let ty = roca_type_to_clif(&p.ty);
        b.param_declare(&p.name, ty, param_vals[i]);
    }

    // Compile body
    for stmt in &func.body {
        if b.is_terminated() { break; }
        compile_stmt(&mut ctx, &mut b, stmt, &func.ret);
    }

    // Emit default return if not terminated
    if !b.is_terminated() {
        let ret_ty = roca_type_to_clif(&func.ret);
        let default_val = match ret_ty {
            ClifType::F64 => b.number(0.0),
            ClifType::I8  => b.bool_val(false),
            ClifType::I64 => b.int_val(0),
        };
        b.return_val(default_val);
    }

    b.finalize();

    let mut cl_ctx = cranelift_codegen::Context::for_function(cl_func);
    if let Err(e) = module.define_function(func_id, &mut cl_ctx) {
        return Err(format!("define_function {func_key}: {e}\nIR:\n{}", cl_ctx.func.display()));
    }

    Ok(())
}

// ─── Statement compilation ────────────────────────────────────────────────────

/// Compile a sequence of statements, stopping early if a terminator is reached.
fn compile_block(ctx: &mut CompileCtx, b: &mut RocaBuilder, stmts: &[Stmt], ret_type: &Type) {
    for s in stmts {
        if b.is_terminated() { break; }
        compile_stmt(ctx, b, s, ret_type);
    }
}

fn compile_stmt(ctx: &mut CompileCtx, b: &mut RocaBuilder, stmt: &Stmt, ret_type: &Type) {
    if b.is_terminated() { return; }

    match stmt {
        Stmt::Let { name, ty, value, is_const: _ } => {
            let val = compile_expr(ctx, b, value);
            let clif_ty = ty.as_ref()
                .map(roca_type_to_clif)
                .unwrap_or_else(|| infer_expr_type(value, b, ctx.struct_reg, ctx.sig_reg));
            b.var_declare(name, clif_ty, val);
            if let Some(sname) = infer_struct_name_from_expr(value, ctx.struct_reg) {
                ctx.var_struct_map.insert(name.clone(), sname);
            }
        }
        Stmt::Var { name, ty, value } => {
            let val = compile_expr(ctx, b, value);
            let clif_ty = ty.as_ref()
                .map(roca_type_to_clif)
                .unwrap_or_else(|| infer_expr_type(value, b, ctx.struct_reg, ctx.sig_reg));
            b.var_declare(name, clif_ty, val);
            if let Some(sname) = infer_struct_name_from_expr(value, ctx.struct_reg) {
                ctx.var_struct_map.insert(name.clone(), sname);
            }
        }
        Stmt::Assign { target, value } => {
            let val = compile_expr(ctx, b, value);
            b.var_set(target, val);
        }
        Stmt::Return(expr) => {
            let val = compile_expr(ctx, b, expr);
            let val = coerce_to_ret(b, val, expr, ret_type, ctx.struct_reg, ctx.sig_reg);
            b.return_val(val);
        }
        Stmt::If { cond, then, else_ } => {
            let cond_val = compile_expr(ctx, b, cond);
            let cond_i8 = coerce_to_i8(b, cond_val, cond, ctx.struct_reg, ctx.sig_reg);

            let then_block = b.create_block();
            let merge_block = b.create_block();

            if let Some(else_stmts) = else_ {
                let else_block = b.create_block();

                b.brif_to(cond_i8, then_block, else_block);

                // then branch
                b.switch_block(then_block);
                b.seal_block(then_block);
                compile_block(ctx, b, then, ret_type);
                if !b.is_terminated() { b.jump_to(merge_block); }

                // else branch
                b.switch_block(else_block);
                b.seal_block(else_block);
                compile_block(ctx, b, else_stmts, ret_type);
                if !b.is_terminated() { b.jump_to(merge_block); }

                // Merge block
                b.switch_block(merge_block);
                b.seal_block(merge_block);
                // If both branches terminated (returned), we're still terminated at merge
                // (merge block has no predecessors → it's dead, but we still switch to it)
            } else {
                b.brif_to(cond_i8, then_block, merge_block);

                // then branch
                b.switch_block(then_block);
                b.seal_block(then_block);
                compile_block(ctx, b, then, ret_type);
                if !b.is_terminated() { b.jump_to(merge_block); }

                b.switch_block(merge_block);
                b.seal_block(merge_block);
            }
        }
        Stmt::Loop { body } => {
            let header = b.create_block();
            let exit = b.create_block();

            b.jump_to(header);
            b.switch_block(header);

            b.loop_stack_push(header, exit);
            compile_block(ctx, b, body, ret_type);
            if !b.is_terminated() {
                b.jump_to(header);
            }
            b.seal_block(header);
            b.loop_stack_pop();

            b.switch_block(exit);
            b.seal_block(exit);
        }
        Stmt::Break => {
            b.break_loop();
        }
        Stmt::Expr(expr) => {
            compile_expr(ctx, b, expr);
        }
        Stmt::SetField { target, field, value } => {
            let ptr = compile_expr(ctx, b, target);
            let val = compile_expr(ctx, b, value);
            let struct_name = match &target.kind {
                ExprKind::SelfRef => ctx.var_struct_map.get("self").cloned(),
                ExprKind::Ident(n) => ctx.var_struct_map.get(n).cloned(),
                _ => None,
            };
            if let Some(sname) = struct_name {
                if let Some(idx) = field_index(ctx.struct_reg, &sname, field) {
                    let idx_val = b.int_val(idx as i64);
                    let val_ty = infer_expr_type(value, b, ctx.struct_reg, ctx.sig_reg);
                    let f64_val = coerce(b, val, val_ty, ClifType::F64);
                    let rt = ctx.import_runtime(b, "mem_struct_set_f64");
                    b.call_imported(rt, &[ptr, idx_val, f64_val]);
                }
            }
        }
        Stmt::ArraySet { .. } => panic!("ArraySet not yet implemented in native compiler"),
        Stmt::For { .. } => panic!("For loop not yet implemented in native compiler"),
        Stmt::Continue => panic!("Continue not yet implemented in native compiler"),
    }
}

// ─── Expression compilation ───────────────────────────────────────────────────

fn compile_expr(ctx: &mut CompileCtx, b: &mut RocaBuilder, expr: &Expr) -> cranelift_codegen::ir::Value {
    match &expr.kind {
        ExprKind::Lit(lit) => compile_lit(ctx, b, lit),
        ExprKind::Ident(name) => b.var_get(name),
        ExprKind::BinOp { op, left, right } => compile_binop(ctx, b, *op, left, right),
        ExprKind::UnaryOp { op, expr } => {
            let val = compile_expr(ctx, b, expr);
            let ty = infer_expr_type(expr, b, ctx.struct_reg, ctx.sig_reg);
            match op {
                UnaryOp::Neg => b.neg(val, ty),
                UnaryOp::Not => b.not(val),
            }
        }
        ExprKind::Call { target, args } => compile_call(ctx, b, target, args),
        ExprKind::GetField { target, field } => compile_get_field(ctx, b, target, field),
        ExprKind::StructLit { name, fields } => compile_struct_lit(ctx, b, name, fields),
        ExprKind::Cast { expr, ty } => {
            let val = compile_expr(ctx, b, expr);
            let from_ty = infer_expr_type(expr, b, ctx.struct_reg, ctx.sig_reg);
            let to_ty = roca_type_to_clif(ty);
            coerce(b, val, from_ty, to_ty)
        }
        ExprKind::SelfRef => b.var_get("self"),

        ExprKind::Match { value, arms } => {
            let scrutinee = compile_expr(ctx, b, value);
            let merge = b.create_block();
            let result_ty = infer_expr_type(&arms[0].body, b, ctx.struct_reg, ctx.sig_reg);
            b.add_block_param(merge, result_ty);

            for (i, arm) in arms.iter().enumerate() {
                let is_last = i == arms.len() - 1;
                match &arm.pattern {
                    roca_lang::Pattern::Wildcard => {
                        let val = compile_expr(ctx, b, &arm.body);
                        b.jump_with(merge, val);
                    }
                    roca_lang::Pattern::Lit(lit) => {
                        let pat = compile_lit(ctx, b, lit);
                        let cond = b.eq(scrutinee, pat, ClifType::I64);
                        let arm_blk = b.create_block();
                        let next = if is_last { merge } else { b.create_block() };
                        b.brif_to(cond, arm_blk, next);

                        b.switch_block(arm_blk);
                        b.seal_block(arm_blk);
                        let val = compile_expr(ctx, b, &arm.body);
                        b.jump_with(merge, val);

                        if !is_last {
                            b.switch_block(next);
                            b.seal_block(next);
                        }
                    }
                    roca_lang::Pattern::Variant { .. } => {
                        panic!("enum variant pattern matching not yet implemented in native compiler")
                    }
                }
            }
            b.seal_block(merge);
            b.switch_block(merge);
            b.block_param(merge, 0)
        }

        ExprKind::If { cond, then, else_ } => {
            let cond_val = compile_expr(ctx, b, cond);
            let cond_i8 = coerce_to_i8(b, cond_val, cond, ctx.struct_reg, ctx.sig_reg);
            let then_blk = b.create_block();
            let else_blk = b.create_block();
            let merge = b.create_block();
            let result_ty = infer_expr_type(then, b, ctx.struct_reg, ctx.sig_reg);
            b.add_block_param(merge, result_ty);
            b.brif_to(cond_i8, then_blk, else_blk);

            b.switch_block(then_blk);
            b.seal_block(then_blk);
            let then_val = compile_expr(ctx, b, then);
            b.jump_with(merge, then_val);

            b.switch_block(else_blk);
            b.seal_block(else_blk);
            let else_val = match else_ {
                Some(e) => compile_expr(ctx, b, e),
                None => b.int_val(0),
            };
            b.jump_with(merge, else_val);

            b.seal_block(merge);
            b.switch_block(merge);
            b.block_param(merge, 0)
        }

        ExprKind::MakeClosure { params, body } => {
            let name = format!("__closure_{}", ctx.closure_counter);
            ctx.closure_counter += 1;

            let mut sig = ctx.module.make_signature();
            for _ in params { sig.params.push(AbiParam::new(types::I64)); }
            sig.returns.push(AbiParam::new(types::I64));

            let fid = ctx.module.declare_function(&name, Linkage::Local, &sig)
                .unwrap_or_else(|e| panic!("declare closure {name}: {e}"));

            let mut cl_func = Function::new();
            cl_func.signature = sig;
            let mut fb_ctx = FunctionBuilderContext::new();
            let mut fb = FunctionBuilder::new(&mut cl_func, &mut fb_ctx);
            let entry = fb.create_block();
            fb.append_block_params_for_function_params(entry);
            fb.switch_to_block(entry);
            fb.seal_block(entry);

            let mut cb = RocaBuilder::new(fb);
            let pvals: Vec<_> = (0..params.len())
                .map(|i| cb.builder.block_params(entry)[i]).collect();
            for (i, pname) in params.iter().enumerate() {
                cb.param_declare(pname, ClifType::I64, pvals[i]);
            }

            let result = compile_expr(ctx, &mut cb, body);
            cb.builder.ins().return_(&[result]);
            cb.finalize();

            let mut cl_ctx = cranelift_codegen::Context::for_function(cl_func);
            ctx.module.define_function(fid, &mut cl_ctx)
                .unwrap_or_else(|e| panic!("define closure {name}: {e}"));

            let fref = ctx.module.declare_func_in_func(fid, b.func_mut());
            b.func_addr(fref)
        }

        ExprKind::Block(stmts, tail) => {
            if let Some(tail_expr) = tail {
                // Block with explicit tail expression
                for stmt in stmts {
                    if b.is_terminated() { break; }
                    compile_stmt(ctx, b, stmt, &Type::Unit);
                }
                compile_expr(ctx, b, tail_expr)
            } else if !stmts.is_empty() {
                // Block ending with an Expr stmt — that's the value
                let (init, last) = stmts.split_at(stmts.len() - 1);
                for stmt in init {
                    if b.is_terminated() { break; }
                    compile_stmt(ctx, b, stmt, &Type::Unit);
                }
                match &last[0] {
                    Stmt::Expr(e) => compile_expr(ctx, b, e),
                    other => {
                        compile_stmt(ctx, b, other, &Type::Unit);
                        b.int_val(0)
                    }
                }
            } else {
                b.int_val(0)
            }
        }

        other => panic!("unimplemented expression in native compiler: {other:?}")
    }
}

fn compile_lit(ctx: &mut CompileCtx, b: &mut RocaBuilder, lit: &Lit) -> cranelift_codegen::ir::Value {
    match lit {
        Lit::Int(n) => b.int_val(*n),
        Lit::Float(f) => b.number(*f),
        Lit::Bool(bv) => b.bool_val(*bv),
        Lit::String(s) => {
            let cstr_ptr = emit_static_cstr(ctx, b, s);
            let func_ref = ctx.import_runtime(b, "mem_string_new");
            let inst = b.builder.ins().call(func_ref, &[cstr_ptr]);
            b.builder.inst_results(inst)[0]
        }
        Lit::Unit => b.int_val(0),
    }
}

fn emit_static_cstr(ctx: &mut CompileCtx, b: &mut RocaBuilder, s: &str) -> cranelift_codegen::ir::Value {
    use cranelift_module::Module;
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0u8);

    let data_id = ctx.module.declare_anonymous_data(false, false)
        .expect("declare data");
    let mut data_desc = cranelift_module::DataDescription::new();
    data_desc.define(bytes.into_boxed_slice());
    ctx.module.define_data(data_id, &data_desc).expect("define data");

    let gv = ctx.module.declare_data_in_func(data_id, b.builder.func);
    b.builder.ins().global_value(types::I64, gv)
}

fn compile_binop(
    ctx: &mut CompileCtx,
    b: &mut RocaBuilder,
    op: BinOp,
    left: &Expr,
    right: &Expr,
) -> cranelift_codegen::ir::Value {
    let lv = compile_expr(ctx, b, left);
    let rv = compile_expr(ctx, b, right);

    let lt = infer_expr_type(left, b, ctx.struct_reg, ctx.sig_reg);
    let rt = infer_expr_type(right, b, ctx.struct_reg, ctx.sig_reg);
    // Dominant type: F64 wins over I64 wins over I8
    let ty = if lt == ClifType::F64 || rt == ClifType::F64 {
        ClifType::F64
    } else if lt == ClifType::I64 || rt == ClifType::I64 {
        ClifType::I64
    } else {
        ClifType::I8
    };

    let lv = coerce(b, lv, lt, ty);
    let rv = coerce(b, rv, rt, ty);

    match op {
        BinOp::Add => b.add(lv, rv, ty),
        BinOp::Sub => b.sub(lv, rv, ty),
        BinOp::Mul => b.mul(lv, rv, ty),
        BinOp::Div => b.div(lv, rv, ty),
        BinOp::Mod => b.mod_op(lv, rv),
        BinOp::Eq  => b.eq(lv, rv, ty),
        BinOp::Ne  => b.ne(lv, rv, ty),
        BinOp::Lt  => b.lt(lv, rv, ty),
        BinOp::Gt  => b.gt(lv, rv, ty),
        BinOp::Le  => b.le(lv, rv, ty),
        BinOp::Ge  => b.ge(lv, rv, ty),
        BinOp::And => b.and(lv, rv),
        BinOp::Or  => b.or(lv, rv),
    }
}

fn compile_call(
    ctx: &mut CompileCtx,
    b: &mut RocaBuilder,
    target: &Expr,
    args: &[Expr],
) -> cranelift_codegen::ir::Value {
    // Resolve the function name and determine if this is an instance method call
    let (func_name, self_val) = resolve_call_target(ctx, b, target);

    // Build args: self first (if instance method), then explicit args
    let mut all_arg_vals: Vec<cranelift_codegen::ir::Value> = Vec::new();
    if let Some(sv) = self_val {
        all_arg_vals.push(sv);
    }
    for a in args {
        all_arg_vals.push(compile_expr(ctx, b, a));
    }

    let coerced_args = all_arg_vals;

    if let Some(&id) = ctx.func_ids.get(&func_name) {
        let func_ref = ctx.module.declare_func_in_func(id, b.builder.func);
        let inst = b.builder.ins().call(func_ref, &coerced_args);
        let results = b.builder.inst_results(inst);
        results.first().copied().unwrap_or_else(|| b.int_val(0))
    } else if b.var_type(&func_name).is_some() {
        // Variable holding a function pointer (closure) — indirect call
        let func_ptr = b.var_get(&func_name);
        // Build signature: all args i64, return i64 (closures are untyped)
        let mut sig = ctx.module.make_signature();
        for _ in &coerced_args { sig.params.push(AbiParam::new(types::I64)); }
        sig.returns.push(AbiParam::new(types::I64));
        let sig_ref = b.builder.import_signature(sig);
        let inst = b.builder.ins().call_indirect(sig_ref, func_ptr, &coerced_args);
        let results = b.builder.inst_results(inst);
        results.first().copied().unwrap_or_else(|| b.int_val(0))
    } else {
        b.int_val(0)
    }
}

/// Resolve a call target to (function_name, Option<self_value>).
/// For `func(args)` → ("func", None)
/// For `Type.method(args)` → ("Type.method", None) — static call
/// For `instance.method(args)` → ("StructType.method", Some(instance_ptr)) — instance call
fn resolve_call_target(
    ctx: &CompileCtx,
    b: &mut RocaBuilder,
    target: &Expr,
) -> (String, Option<cranelift_codegen::ir::Value>) {
    match &target.kind {
        ExprKind::Ident(name) => (name.clone(), None),
        ExprKind::GetField { target: obj, field } => {
            match &obj.kind {
                ExprKind::Ident(name) => {
                    // Is `name` a type name (static call) or a variable (instance call)?
                    let qualified = format!("{name}.{field}");
                    if ctx.func_ids.contains_key(&qualified) {
                        // Could be static — check if it's also a variable
                        if let Some(struct_type) = ctx.var_struct_map.get(name) {
                            // It's a variable with a known struct type → instance call
                            let self_ptr = b.var_get(name);
                            let method_name = format!("{struct_type}.{field}");
                            (method_name, Some(self_ptr))
                        } else {
                            // Type-level static call: Point.new(...)
                            (qualified, None)
                        }
                    } else if let Some(struct_type) = ctx.var_struct_map.get(name) {
                        // Variable with struct type → instance method call
                        let self_ptr = b.var_get(name);
                        let method_name = format!("{struct_type}.{field}");
                        (method_name, Some(self_ptr))
                    } else {
                        (qualified, None)
                    }
                }
                _ => {
                    let base = match &obj.kind {
                        ExprKind::Ident(n) => n.clone(),
                        _ => "unknown".to_string(),
                    };
                    (format!("{base}.{field}"), None)
                }
            }
        }
        _ => ("unknown".to_string(), None),
    }
}

fn compile_get_field(
    ctx: &mut CompileCtx,
    b: &mut RocaBuilder,
    target: &Expr,
    field: &str,
) -> cranelift_codegen::ir::Value {
    let struct_name = infer_struct_name(target, ctx.struct_reg, &ctx.var_struct_map);
    let ptr = compile_expr(ctx, b, target);

    if let Some(sname) = struct_name {
        if let (Some(idx), Some(fty)) = (
            field_index(ctx.struct_reg, &sname, field),
            field_type(ctx.struct_reg, &sname, field),
        ) {
            let idx_val = b.int_val(idx as i64);
            let func_ref = ctx.import_runtime(b, "mem_struct_get_f64");
            let inst = b.builder.ins().call(func_ref, &[ptr, idx_val]);
            let f64_val = b.builder.inst_results(inst)[0];
            coerce(b, f64_val, ClifType::F64, roca_type_to_clif(&fty))
        } else {
            panic!("compiler bug: struct '{sname}' has no field '{field}' — checker should have rejected this")
        }
    } else {
        // Unknown struct — return f64 as-is
        let idx_val = b.int_val(0);
        let func_ref = ctx.import_runtime(b, "mem_struct_get_f64");
        let inst = b.builder.ins().call(func_ref, &[ptr, idx_val]);
        b.builder.inst_results(inst)[0]
    }
}

fn compile_struct_lit(
    ctx: &mut CompileCtx,
    b: &mut RocaBuilder,
    name: &str,
    fields: &[(String, Expr)],
) -> cranelift_codegen::ir::Value {
    let n_fields = fields.len() as i64;
    let type_id = roca_mem::name_to_type_id(name) as i64;

    let n_val = b.int_val(n_fields);
    let tid_val = b.int_val(type_id);

    let struct_new_ref = ctx.import_runtime(b, "mem_struct_new");
    let inst = b.builder.ins().call(struct_new_ref, &[n_val, tid_val]);
    let ptr = b.builder.inst_results(inst)[0];

    // Set each field — store everything as f64 bits for uniformity
    for (field_name, field_expr) in fields {
        let idx = if let Some(reg_fields) = ctx.struct_reg.get(name) {
            reg_fields.iter().position(|(n, _)| n == field_name)
                .unwrap_or(0) as i64
        } else {
            0i64
        };

        let val = compile_expr(ctx, b, field_expr);
        let idx_val = b.int_val(idx);

        let from_ty = infer_expr_type(field_expr, b, ctx.struct_reg, ctx.sig_reg);
        let f64_val = coerce(b, val, from_ty, ClifType::F64);

        let func_ref = ctx.import_runtime(b, "mem_struct_set_f64");
        b.builder.ins().call(func_ref, &[ptr, idx_val, f64_val]);
    }

    ptr
}

/// Simple call name resolution for type inference (no CompileCtx needed).
fn simple_call_name(target: &Expr) -> String {
    match &target.kind {
        ExprKind::Ident(name) => name.clone(),
        ExprKind::GetField { target, field } => {
            let base = simple_call_name(target);
            format!("{base}.{field}")
        }
        _ => "unknown".to_string(),
    }
}

// ─── Type inference helpers ───────────────────────────────────────────────────

fn infer_expr_type(expr: &Expr, b: &RocaBuilder, struct_reg: &StructRegistry, sig_reg: &SigRegistry) -> ClifType {
    match &expr.kind {
        ExprKind::Lit(Lit::Int(_))    => ClifType::I64,
        ExprKind::Lit(Lit::Float(_))  => ClifType::F64,
        ExprKind::Lit(Lit::Bool(_))   => ClifType::I8,
        ExprKind::Lit(Lit::String(_)) => ClifType::I64,
        ExprKind::Lit(Lit::Unit)      => ClifType::I64,
        ExprKind::BinOp { op, left, right } => {
            match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                    let lt = infer_expr_type(left, b, struct_reg, sig_reg);
                    let rt = infer_expr_type(right, b, struct_reg, sig_reg);
                    if lt == ClifType::F64 || rt == ClifType::F64 { ClifType::F64 } else { lt }
                }
                _ => ClifType::I8, // comparisons → bool
            }
        }
        ExprKind::Call { target, .. } => {
            let name = simple_call_name(target);
            sig_reg.get(&name).map(|(_, ret)| *ret).unwrap_or(ClifType::I64)
        }
        ExprKind::Ident(name) => b.var_type(name).unwrap_or(ClifType::I64),
        ExprKind::GetField { .. } => ClifType::I64, // conservative
        ExprKind::StructLit { .. } => ClifType::I64,
        _ => ClifType::I64,
    }
}

fn infer_struct_name(
    expr: &Expr,
    struct_reg: &StructRegistry,
    var_struct_map: &HashMap<String, String>,
) -> Option<String> {
    match &expr.kind {
        ExprKind::SelfRef => var_struct_map.get("self").cloned(),
        ExprKind::Ident(name) => {
            if let Some(sname) = var_struct_map.get(name) {
                return Some(sname.clone());
            }
            if struct_reg.contains_key(name) {
                Some(name.clone())
            } else {
                None
            }
        }
        ExprKind::Call { target, .. } => {
            let fn_name = simple_call_name(target);
            if fn_name.contains('.') {
                let struct_name = fn_name.split('.').next().unwrap_or("").to_string();
                if struct_reg.contains_key(&struct_name) {
                    return Some(struct_name);
                }
            }
            None
        }
        _ => None,
    }
}

fn infer_struct_name_from_expr(expr: &Expr, struct_reg: &StructRegistry) -> Option<String> {
    match &expr.kind {
        ExprKind::Call { target, .. } => {
            let fn_name = simple_call_name(target);
            if fn_name.contains('.') {
                let struct_name = fn_name.split('.').next().unwrap_or("").to_string();
                if struct_reg.contains_key(&struct_name) {
                    return Some(struct_name);
                }
            }
            None
        }
        ExprKind::StructLit { name, .. } => Some(name.clone()),
        _ => None,
    }
}

// ─── Coercion helpers ─────────────────────────────────────────────────────────

fn coerce(b: &mut RocaBuilder, val: cranelift_codegen::ir::Value, from: ClifType, to: ClifType) -> cranelift_codegen::ir::Value {
    if from == to { return val; }
    match (from, to) {
        (ClifType::I8, ClifType::I64) => b.widen_to_i64(val),
        (ClifType::I64, ClifType::I8) => b.narrow_to_i8(val),
        (ClifType::I64, ClifType::F64) => b.i64_to_f64(val),
        (ClifType::F64, ClifType::I64) => b.f64_to_i64(val),
        (ClifType::I8, ClifType::F64)  => {
            let wide = b.widen_to_i64(val);
            b.i64_to_f64(wide)
        }
        (ClifType::F64, ClifType::I8) => {
            let i64v = b.f64_to_i64(val);
            b.narrow_to_i8(i64v)
        }
        _ => val,
    }
}

fn coerce_to_i8(b: &mut RocaBuilder, val: cranelift_codegen::ir::Value, expr: &Expr, struct_reg: &StructRegistry, sig_reg: &SigRegistry) -> cranelift_codegen::ir::Value {
    let ty = infer_expr_type(expr, b, struct_reg, sig_reg);
    coerce(b, val, ty, ClifType::I8)
}

fn coerce_to_ret(
    b: &mut RocaBuilder,
    val: cranelift_codegen::ir::Value,
    expr: &Expr,
    ret_type: &Type,
    struct_reg: &StructRegistry,
    sig_reg: &SigRegistry,
) -> cranelift_codegen::ir::Value {
    let from = infer_expr_type(expr, b, struct_reg, sig_reg);
    let to = roca_type_to_clif(ret_type);
    coerce(b, val, from, to)
}
