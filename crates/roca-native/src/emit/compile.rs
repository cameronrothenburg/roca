//! Top-level compilation: function declaration, closure pre-compilation, function/method bodies.
//! Uses Function/Struct/Satisfies/ExternFn/ExternContract builders from roca-cranelift.

use std::collections::HashMap;
use roca_ast::{self as roca};
use roca_cranelift::api::Function;
use roca_cranelift::{Module, FuncId, FnDecl, CompiledFuncs, Value as CraneliftValue};
use roca_types::RocaType;
use crate::runtime::RuntimeFuncs;
use super::context::NativeCtx;
use super::emit::{emit_body, emit_expr, closure_hash};

// ─── Metadata extraction ─────────────────────────────

/// Build a map of function name → return RocaType from the source file.
pub fn build_return_kind_map(source: &roca::SourceFile) -> HashMap<String, RocaType> {
    let mut map = HashMap::new();
    for item in &source.items {
        match item {
            roca::Item::Function(f) => {
                map.insert(f.name.clone(), RocaType::from(&f.return_type));
            }
            roca::Item::ExternFn(ef) => {
                map.insert(ef.name.clone(), RocaType::from(&ef.return_type));
            }
            roca::Item::ExternContract(c) => {
                for sig in &c.functions {
                    map.insert(format!("{}.{}", c.name, sig.name), RocaType::from(&sig.return_type));
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

// ─── Forward declaration ─────────────────────────────

/// Declare all functions in the module (signatures only, no bodies).
/// This enables forward references — any function can call any other.
pub fn declare_all_functions<M: Module>(
    module: &mut M,
    source: &roca::SourceFile,
    compiled: &mut CompiledFuncs,
) -> Result<(), String> {
    let mut declarations = Vec::new();

    for item in &source.items {
        let fns_to_declare: Vec<(&roca::FnDef, Option<&str>)> = match item {
            roca::Item::Function(f) => vec![(f, None)],
            roca::Item::Struct(s) => s.methods.iter().map(|m| (m, Some(s.name.as_str()))).collect(),
            roca::Item::Satisfies(sat) => sat.methods.iter().map(|m| (m, Some(sat.struct_name.as_str()))).collect(),
            _ => vec![],
        };
        for (f, struct_name) in fns_to_declare {
            let name = if let Some(sn) = struct_name {
                format!("{}.{}", sn, f.name)
            } else {
                f.name.clone()
            };
            declarations.push(FnDecl {
                name,
                params: f.params.iter().map(|p| RocaType::from(&p.type_ref)).collect(),
                has_self: struct_name.is_some(),
                return_type: RocaType::from(&f.return_type),
                returns_err: f.returns_err,
            });
        }
    }

    roca_cranelift::declare_functions(module, &declarations, compiled)
}

// ─── Closure pre-compilation ─────────────────────────

/// Pre-compile all closures in a source file as top-level functions.
pub fn compile_closures<M: Module>(
    module: &mut M,
    source: &roca::SourceFile,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    func_return_kinds: &HashMap<String, RocaType>,
) -> Result<(), String> {
    let mut closures = Vec::new();
    for item in &source.items {
        if let roca::Item::Function(f) = item {
            collect_closures(&f.body, &mut closures);
        }
    }
    for (params, closure_body) in closures {
        let name = format!("__closure_{}_{}", params.len(), closure_hash(&params, &closure_body));
        if compiled.has(&name) { continue; }

        let mut func = Function::new(&name);
        for p in &params {
            func = func.param(p, RocaType::Number);
        }
        func = func.returns(RocaType::Number);

        let nctx = NativeCtx {
            func_return_kinds: func_return_kinds.clone(),
            ..Default::default()
        };
        func.build(module, rt, compiled, |body| {
            let val = emit_expr(body, &nctx, &closure_body);
            body.return_val(val);
        })?;
    }
    Ok(())
}

fn collect_closures(stmts: &[roca::Stmt], out: &mut Vec<(Vec<String>, roca::Expr)>) {
    for stmt in stmts {
        match stmt {
            roca::Stmt::Const { value, .. } | roca::Stmt::Let { value, .. } => {
                if let roca::Expr::Closure { params, body } = value {
                    out.push((params.clone(), *body.clone()));
                }
                collect_closures_from_call_args(value, out);
            }
            roca::Stmt::Return(expr) | roca::Stmt::Expr(expr) => {
                collect_closures_from_call_args(expr, out);
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

/// Collect closures passed as arguments to function/method calls (e.g. `arr.map(|x| x + 1)`).
/// Skip closures in map/filter calls — those are always inlined by emit_method_call.
fn collect_closures_from_call_args(expr: &roca::Expr, out: &mut Vec<(Vec<String>, roca::Expr)>) {
    if let roca::Expr::Call { target, args } = expr {
        let is_inlined_method = matches!(
            target.as_ref(),
            roca::Expr::FieldAccess { field, .. } if field == "map" || field == "filter"
        );
        if !is_inlined_method && matches!(target.as_ref(), roca::Expr::Ident(_) | roca::Expr::FieldAccess { .. }) {
            for a in args {
                if let roca::Expr::Closure { params, body } = a {
                    out.push((params.clone(), *body.clone()));
                }
            }
        }
    }
}

// ─── Wait expression pre-compilation ─────────────────

/// Pre-compile wait expressions as zero-arg functions.
pub fn compile_wait_exprs<M: Module>(
    module: &mut M,
    source: &roca::SourceFile,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    func_return_kinds: &HashMap<String, RocaType>,
) -> Result<(), String> {
    let mut wait_exprs = Vec::new();
    for item in &source.items {
        if let roca::Item::Function(f) = item {
            collect_wait_exprs(&f.body, &mut wait_exprs);
        }
    }
    let nctx = NativeCtx {
        func_return_kinds: func_return_kinds.clone(),
        ..Default::default()
    };
    for (name, expr) in wait_exprs {
        if compiled.has(&name) { continue; }
        Function::new(&name)
            .returns(RocaType::Number)
            .build(module, rt, compiled, |body| {
                let val = emit_expr(body, &nctx, &expr);
                body.return_val(val);
            })?;
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

pub(super) fn expr_debug_hash(expr: &roca::Expr) -> u64 {
    use std::hash::{Hash, Hasher, DefaultHasher};
    let mut h = DefaultHasher::new();
    format!("{:?}", expr).hash(&mut h);
    h.finish()
}

// ─── NativeCtx construction ─────────────────────────

/// Build a NativeCtx from the Roca-specific metadata for a function.
fn build_native_ctx(
    crash: Option<&roca::CrashBlock>,
    func_return_kinds: &HashMap<String, RocaType>,
    enum_variants: &HashMap<String, Vec<String>>,
    struct_defs: &HashMap<String, Vec<roca::Field>>,
) -> NativeCtx {
    let mut nctx = NativeCtx {
        func_return_kinds: func_return_kinds.clone(),
        enum_variants: enum_variants.clone(),
        struct_defs: struct_defs.clone(),
        ..Default::default()
    };
    if let Some(crash_block) = crash {
        for h in &crash_block.handlers {
            nctx.crash_handlers.insert(h.call.clone(), h.strategy.clone());
        }
    }
    nctx
}

// ─── Function compilation ────────────────────────────

/// Compile a Roca function to native code.
pub fn compile_function<M: Module>(
    module: &mut M,
    func: &roca::FnDef,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    func_return_kinds: &HashMap<String, RocaType>,
    enum_variants: &HashMap<String, Vec<String>>,
    struct_defs: &HashMap<String, Vec<roca::Field>>,
) -> Result<FuncId, String> {
    let mut f = Function::new(&func.name);
    for p in &func.params {
        f = f.param_with_constraints(&p.name, RocaType::from(&p.type_ref),
            p.constraints.iter().map(roca_types::Constraint::from).collect());
    }
    f = f.returns(RocaType::from(&func.return_type))
        .returns_err_if(func.returns_err);

    let nctx = build_native_ctx(func.crash.as_ref(), func_return_kinds, enum_variants, struct_defs);
    let body_stmts = func.body.clone();
    let params = func.params.clone();
    f.build(module, rt, compiled, |body| {
        super::emit::emit_param_constraints(body, &params);
        emit_body(body, &nctx, &body_stmts);
    })
}

/// Compile a struct method.
pub fn compile_struct_method<M: Module>(
    module: &mut M,
    func: &roca::FnDef,
    struct_name: &str,
    fields: &[roca::Field],
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
    func_return_kinds: &HashMap<String, RocaType>,
    enum_variants: &HashMap<String, Vec<String>>,
    struct_defs: &HashMap<String, Vec<roca::Field>>,
) -> Result<FuncId, String> {
    let field_info: Vec<(String, RocaType)> = fields.iter()
        .map(|f| (f.name.clone(), RocaType::from(&f.type_ref)))
        .collect();

    let mut f = Function::new(&format!("{}.{}", struct_name, func.name))
        .self_param();
    for p in &func.params {
        f = f.param_with_constraints(&p.name, RocaType::from(&p.type_ref),
            p.constraints.iter().map(roca_types::Constraint::from).collect());
    }
    f = f.returns(RocaType::from(&func.return_type))
        .returns_err_if(func.returns_err)
        .with_struct_layout(struct_name, roca_cranelift::StructLayout::new(field_info))
        .with_self_struct_type(struct_name);

    let nctx = build_native_ctx(func.crash.as_ref(), func_return_kinds, enum_variants, struct_defs);
    let body_stmts = func.body.clone();
    let params = func.params.clone();
    f.build(module, rt, compiled, |body| {
        super::emit::emit_param_constraints(body, &params);
        emit_body(body, &nctx, &body_stmts);
    })
}

/// Compile an auto-stub for an extern fn.
pub fn compile_extern_fn_stub<M: Module>(
    module: &mut M,
    extern_fn: &roca::ExternFnDef,
    default_value_expr: &roca::Expr,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
) -> Result<FuncId, String> {
    let mut f = Function::new(&extern_fn.name);
    for p in &extern_fn.params {
        f = f.param(&p.name, RocaType::from(&p.type_ref));
    }
    f = f.returns(RocaType::from(&extern_fn.return_type))
        .returns_err_if(extern_fn.returns_err);

    let default_expr = default_value_expr.clone();
    let nctx = NativeCtx::default();
    f.build(module, rt, compiled, |body| {
        let val = emit_expr(body, &nctx, &default_expr);
        body.return_val(val);
    })
}

/// Compile a test shim for a function.
///
/// The shim has a fixed 1-param calling convention: `fn(args_ptr: i64) -> i64`
/// (or `-> (i64, i8)` for error-returning functions). The test runner packs all args into a
/// `Vec<u64>` and passes a pointer — one code path for any param count and any type mix.
///
/// The shim unpacks each arg from the array (loading 8 bytes at each slot), converts to the
/// real param type (bitcast for f64, narrow for bool, identity for string/struct), then calls
/// the real function and returns the result unified as i64.
pub fn compile_test_shim<M: Module>(
    module: &mut M,
    func: &roca::FnDef,
    struct_name: Option<&str>,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
) -> Result<(), String> {
    let real_name = match struct_name {
        Some(sn) => format!("{}.{}", sn, func.name),
        None => func.name.clone(),
    };
    let shim_name = crate::test_runner::shim_name(&real_name);
    if compiled.has(&shim_name) { return Ok(()); }

    let param_types: Vec<RocaType> = func.params.iter()
        .map(|p| RocaType::from(&p.type_ref))
        .collect();
    let ret_type = RocaType::from(&func.return_type);
    let returns_err = func.returns_err;
    let has_self = struct_name.is_some();

    // Shim signature: (args_ptr: I64) -> I64 [+ I8 if err]
    // RocaType::String maps to I64 — used for the pointer param and the unified i64 return.
    Function::new(&shim_name)
        .param("args_ptr", RocaType::String)
        .returns(RocaType::String)
        .returns_err_if(returns_err)
        .build(module, rt, compiled, move |body| {
            let args_ptr = body.var("args_ptr");

            // Unpack args from the array. Layout: [self?, param0, param1, ...]
            let mut call_args: Vec<CraneliftValue> = Vec::new();
            let mut slot = 0i32;

            if has_self {
                call_args.push(body.load_ptr_i64(args_ptr, slot));
                slot += 8;
            }

            for pt in &param_types {
                let bits = body.load_ptr_i64(args_ptr, slot);
                let arg = match pt {
                    RocaType::Number => body.bitcast_i64_to_f64(bits),
                    RocaType::Bool   => body.narrow_to_bool(bits),
                    _                => bits,
                };
                call_args.push(arg);
                slot += 8;
            }

            // Unify raw result to i64 for the shim's uniform return type.
            macro_rules! unify_to_i64 {
                ($body:expr, $raw:expr) => {
                    match ret_type {
                        RocaType::Number => $body.bitcast_f64_to_i64($raw),
                        RocaType::Bool   => $body.extend_bool($raw),
                        _                => $raw,
                    }
                }
            }

            if returns_err {
                let results = body.call_multi(&real_name, &call_args);
                let (raw_result, err_tag) = if results.len() >= 2 {
                    (results[0], results[1])
                } else {
                    (body.int(0), body.bool_val(false))
                };
                let result_i64 = unify_to_i64!(body, raw_result);
                body.return_with_err_val(result_i64, err_tag);
            } else {
                let raw_result = body.call(&real_name, &call_args);
                let result_i64 = unify_to_i64!(body, raw_result);
                body.return_val(result_i64);
            }
        })?;

    Ok(())
}

/// Compile auto-stubs for all methods in an extern contract.
pub fn compile_contract_stubs<M: Module>(
    module: &mut M,
    contract: &roca::ContractDef,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
) -> Result<(), String> {
    for sig_def in &contract.functions {
        let qualified = format!("{}.{}", contract.name, sig_def.name);
        if compiled.has(&qualified) { continue; }

        let ret_type = RocaType::from(&sig_def.return_type);
        let returns_err = sig_def.returns_err;
        let mut f = Function::new(&qualified);
        for p in &sig_def.params {
            f = f.param(&p.name, RocaType::from(&p.type_ref));
        }
        f = f.returns(ret_type.clone()).returns_err_if(returns_err);

        let ret_type_clone = ret_type;
        let result = f.build(module, rt, compiled, |body| {
            let dv = body.default_for(&ret_type_clone);
            body.return_val(dv);
        });

        if result.is_err() {
            // Skip stubs that fail to compile (e.g., generic params)
            continue;
        }
    }
    Ok(())
}
