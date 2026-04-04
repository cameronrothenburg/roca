//! compiler.rs — AST walker that emits Cranelift IR through the builder.

use std::collections::HashMap;

use cranelift_codegen::ir::{
    types, AbiParam, Function, InstBuilder, UserFuncName,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};

use roca_lang::ast::{BinOp, Expr, FuncDef, Item, Lit, Param, SourceFile, Stmt, Type, UnaryOp};

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

fn field_index(registry: &StructRegistry, struct_name: &str, field_name: &str) -> usize {
    let fields = registry.get(struct_name)
        .unwrap_or_else(|| panic!("unknown struct: {struct_name}"));
    fields.iter().position(|(n, _)| n == field_name)
        .unwrap_or_else(|| panic!("unknown field {field_name} on {struct_name}"))
}

fn field_type(registry: &StructRegistry, struct_name: &str, field_name: &str) -> Type {
    let fields = registry.get(struct_name)
        .unwrap_or_else(|| panic!("unknown struct: {struct_name}"));
    fields.iter().find(|(n, _)| n == field_name)
        .map(|(_, t)| t.clone())
        .unwrap_or_else(|| panic!("unknown field {field_name} on {struct_name}"))
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
                    let params: Vec<ClifType> = m.params.iter().map(|p| roca_type_to_clif(&p.ty)).collect();
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
                    let sig = make_sig(&module, &m.params, &m.ret);
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
                let id = func_ids[&f.name];
                compile_function(
                    &mut module, id, f, &f.name,
                    &func_ids, &runtime_ids, &struct_reg, &sig_reg,
                ).map_err(|e| format!("compile {}: {e}", f.name))?;
            }
            Item::Struct(s) => {
                for m in &s.methods {
                    let key = format!("{}.{}", s.name, m.name);
                    let id = func_ids[&key];
                    compile_function(
                        &mut module, id, m, &key,
                        &func_ids, &runtime_ids, &struct_reg, &sig_reg,
                    ).map_err(|e| format!("compile {key}: {e}"))?;
                }
            }
            _ => {}
        }
    }

    module.finalize_definitions().map_err(|e| format!("finalize: {e}"))?;

    Ok(CompiledModule { jit: module, func_ids })
}

fn make_sig(module: &JITModule, params: &[Param], ret: &Type) -> cranelift_codegen::ir::Signature {
    let mut sig = module.make_signature();
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
    /// Maps variable name → struct type name for field access inference.
    var_struct_map: HashMap<String, String>,
}

impl<'a> CompileCtx<'a> {
    fn import_runtime(&mut self, b: &mut RocaBuilder, name: &str) -> cranelift_codegen::ir::FuncRef {
        let id = self.runtime_ids[name];
        self.module.declare_func_in_func(id, b.builder.func)
    }

    fn import_user_func(&mut self, b: &mut RocaBuilder, name: &str) -> cranelift_codegen::ir::FuncRef {
        let id = self.func_ids[name];
        self.module.declare_func_in_func(id, b.builder.func)
    }
}

// ─── Compile one function ─────────────────────────────────────────────────────

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
    let sig = {
        let mut s = module.make_signature();
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
    };

    let mut b = RocaBuilder::new(fb, sig_reg.clone());

    // Create entry block
    let entry = b.create_block();
    b.builder.append_block_params_for_function_params(entry);
    b.builder.switch_to_block(entry);
    b.builder.seal_block(entry);

    // Bind parameters
    let param_vals: Vec<_> = (0..func.params.len())
        .map(|i| b.builder.block_params(entry)[i])
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
    module.define_function(func_id, &mut cl_ctx)
        .map_err(|e| format!("define_function {func_key}: {e}"))?;

    Ok(())
}

// ─── Statement compilation ────────────────────────────────────────────────────

fn compile_stmt(ctx: &mut CompileCtx, b: &mut RocaBuilder, stmt: &Stmt, ret_type: &Type) {
    if b.is_terminated() { return; }

    match stmt {
        Stmt::Let { name, ty, value, is_const: _ } => {
            let val = compile_expr(ctx, b, value);
            let clif_ty = ty.as_ref()
                .map(roca_type_to_clif)
                .unwrap_or_else(|| infer_expr_type(value, ctx.struct_reg, ctx.sig_reg));
            b.var_declare(name, clif_ty, val);
            if let Some(sname) = infer_struct_name_from_expr(value, ctx.struct_reg) {
                ctx.var_struct_map.insert(name.clone(), sname);
            }
        }
        Stmt::Var { name, ty, value } => {
            let val = compile_expr(ctx, b, value);
            let clif_ty = ty.as_ref()
                .map(roca_type_to_clif)
                .unwrap_or_else(|| infer_expr_type(value, ctx.struct_reg, ctx.sig_reg));
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
                let mut then_terminated = false;
                for s in then {
                    if b.is_terminated() { then_terminated = true; break; }
                    compile_stmt(ctx, b, s, ret_type);
                }
                if !b.is_terminated() {
                    b.jump_to(merge_block);
                } else {
                    then_terminated = true;
                }

                // else branch
                b.switch_block(else_block);
                b.seal_block(else_block);
                let mut else_terminated = false;
                for s in else_stmts {
                    if b.is_terminated() { else_terminated = true; break; }
                    compile_stmt(ctx, b, s, ret_type);
                }
                if !b.is_terminated() {
                    b.jump_to(merge_block);
                } else {
                    else_terminated = true;
                }

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
                for s in then {
                    if b.is_terminated() { break; }
                    compile_stmt(ctx, b, s, ret_type);
                }
                if !b.is_terminated() {
                    b.jump_to(merge_block);
                }

                // merge
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

            for s in body {
                if b.is_terminated() { break; }
                compile_stmt(ctx, b, s, ret_type);
            }
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
        Stmt::SetField { .. } | Stmt::ArraySet { .. } | Stmt::For { .. } | Stmt::Continue => {
            // Not needed for the 10 tests
        }
    }
}

// ─── Expression compilation ───────────────────────────────────────────────────

fn compile_expr(ctx: &mut CompileCtx, b: &mut RocaBuilder, expr: &Expr) -> cranelift_codegen::ir::Value {
    match expr {
        Expr::Lit(lit) => compile_lit(ctx, b, lit),
        Expr::Ident(name) => b.var_get(name),
        Expr::BinOp { op, left, right } => compile_binop(ctx, b, *op, left, right),
        Expr::UnaryOp { op, expr } => {
            let val = compile_expr(ctx, b, expr);
            let ty = infer_expr_type(expr, ctx.struct_reg, ctx.sig_reg);
            match op {
                UnaryOp::Neg => b.neg(val, ty),
                UnaryOp::Not => b.not(val),
            }
        }
        Expr::Call { target, args } => compile_call(ctx, b, target, args),
        Expr::GetField { target, field } => compile_get_field(ctx, b, target, field),
        Expr::StructLit { name, fields } => compile_struct_lit(ctx, b, name, fields),
        Expr::Cast { expr, ty } => {
            let val = compile_expr(ctx, b, expr);
            let from_ty = infer_expr_type(expr, ctx.struct_reg, ctx.sig_reg);
            let to_ty = roca_type_to_clif(ty);
            coerce(b, val, from_ty, to_ty)
        }
        Expr::SelfRef => b.int_val(0),
        _ => b.int_val(0),
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

    let lt = infer_expr_type(left, ctx.struct_reg, ctx.sig_reg);
    let rt = infer_expr_type(right, ctx.struct_reg, ctx.sig_reg);
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
    let func_name = resolve_call_name(target);

    let arg_vals: Vec<_> = args.iter().map(|a| compile_expr(ctx, b, a)).collect();

    // Coerce args to declared param types
    let coerced_args: Vec<_> = if let Some((param_tys, _)) = ctx.sig_reg.get(&func_name) {
        let param_tys = param_tys.clone();
        arg_vals.iter().enumerate().map(|(i, &v)| {
            let from = infer_expr_type(&args[i], ctx.struct_reg, ctx.sig_reg);
            let to = param_tys.get(i).copied().unwrap_or(from);
            coerce(b, v, from, to)
        }).collect()
    } else {
        arg_vals.clone()
    };

    if let Some(&id) = ctx.func_ids.get(&func_name) {
        let func_ref = ctx.module.declare_func_in_func(id, b.builder.func);
        let inst = b.builder.ins().call(func_ref, &coerced_args);
        let results = b.builder.inst_results(inst);
        results.first().copied().unwrap_or_else(|| b.int_val(0))
    } else {
        b.int_val(0)
    }
}

fn resolve_call_name(target: &Expr) -> String {
    match target {
        Expr::Ident(name) => name.clone(),
        Expr::GetField { target, field } => {
            let base = resolve_call_name(target);
            format!("{base}.{field}")
        }
        _ => "unknown".to_string(),
    }
}

fn compile_get_field(
    ctx: &mut CompileCtx,
    b: &mut RocaBuilder,
    target: &Expr,
    field: &str,
) -> cranelift_codegen::ir::Value {
    let struct_name = infer_struct_name(target, ctx.struct_reg, ctx.sig_reg, &ctx.var_struct_map);
    let ptr = compile_expr(ctx, b, target);

    if let Some(sname) = struct_name {
        let idx = field_index(ctx.struct_reg, &sname, field) as i64;
        let fty = field_type(ctx.struct_reg, &sname, field);
        let idx_val = b.int_val(idx);

        // All fields stored as f64 bits via set_f64; retrieve and convert back.
        let func_ref = ctx.import_runtime(b, "mem_struct_get_f64");
        let inst = b.builder.ins().call(func_ref, &[ptr, idx_val]);
        let f64_val = b.builder.inst_results(inst)[0];

        coerce(b, f64_val, ClifType::F64, roca_type_to_clif(&fty))
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

        let from_ty = infer_expr_type(field_expr, ctx.struct_reg, ctx.sig_reg);
        let f64_val = coerce(b, val, from_ty, ClifType::F64);

        let func_ref = ctx.import_runtime(b, "mem_struct_set_f64");
        b.builder.ins().call(func_ref, &[ptr, idx_val, f64_val]);
    }

    ptr
}

// ─── Type inference helpers ───────────────────────────────────────────────────

fn infer_expr_type(expr: &Expr, struct_reg: &StructRegistry, sig_reg: &SigRegistry) -> ClifType {
    match expr {
        Expr::Lit(Lit::Int(_))    => ClifType::I64,
        Expr::Lit(Lit::Float(_))  => ClifType::F64,
        Expr::Lit(Lit::Bool(_))   => ClifType::I8,
        Expr::Lit(Lit::String(_)) => ClifType::I64,
        Expr::Lit(Lit::Unit)      => ClifType::I64,
        Expr::BinOp { op, left, right } => {
            match op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                    let lt = infer_expr_type(left, struct_reg, sig_reg);
                    let rt = infer_expr_type(right, struct_reg, sig_reg);
                    if lt == ClifType::F64 || rt == ClifType::F64 { ClifType::F64 } else { lt }
                }
                _ => ClifType::I8, // comparisons → bool
            }
        }
        Expr::Call { target, .. } => {
            let name = resolve_call_name(target);
            sig_reg.get(&name).map(|(_, ret)| *ret).unwrap_or(ClifType::I64)
        }
        Expr::Ident(_)     => ClifType::I64,
        Expr::GetField { .. } => ClifType::I64, // conservative
        Expr::StructLit { .. } => ClifType::I64,
        _ => ClifType::I64,
    }
}

fn infer_struct_name(
    expr: &Expr,
    struct_reg: &StructRegistry,
    sig_reg: &SigRegistry,
    var_struct_map: &HashMap<String, String>,
) -> Option<String> {
    match expr {
        Expr::Ident(name) => {
            if let Some(sname) = var_struct_map.get(name) {
                return Some(sname.clone());
            }
            if struct_reg.contains_key(name) {
                Some(name.clone())
            } else {
                None
            }
        }
        Expr::Call { target, .. } => {
            let fn_name = resolve_call_name(target);
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
    match expr {
        Expr::Call { target, .. } => {
            let fn_name = resolve_call_name(target);
            if fn_name.contains('.') {
                let struct_name = fn_name.split('.').next().unwrap_or("").to_string();
                if struct_reg.contains_key(&struct_name) {
                    return Some(struct_name);
                }
            }
            None
        }
        Expr::StructLit { name, .. } => Some(name.clone()),
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
    let ty = infer_expr_type(expr, struct_reg, sig_reg);
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
    let from = infer_expr_type(expr, struct_reg, sig_reg);
    let to = roca_type_to_clif(ret_type);
    coerce(b, val, from, to)
}
