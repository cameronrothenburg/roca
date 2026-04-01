//! Top-level compilation: function declaration, closure pre-compilation, function/method bodies.

use std::collections::HashMap;
use cranelift_codegen::ir::types;
use cranelift_module::{Module, FuncId};

use roca_ast::{self as roca};
use roca_cranelift::api::Function;
use roca_cranelift::builder::{FunctionCompiler, FunctionSpec};
use roca_cranelift::cranelift_type::CraneliftType;
use roca_types::RocaType;
use crate::runtime::RuntimeFuncs;
use roca_cranelift::context::{CompiledFuncs, EmitCtx, ValKind, StructLayout, VarInfo};
use super::helpers::emit_scope_cleanup;
use super::expr::emit_expr;
use super::stmt::emit_stmt;
use super::methods::emit_param_constraints;

/// Build a map of function name → return ValKind from the source file.
pub fn build_return_kind_map(source: &roca::SourceFile) -> HashMap<String, ValKind> {
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

/// Declare all functions in the module (signatures only, no bodies).
/// This enables forward references — any function can call any other.
pub fn declare_all_functions<M: Module>(
    module: &mut M,
    source: &roca::SourceFile,
    compiled: &mut CompiledFuncs,
) -> Result<(), String> {
    use cranelift_codegen::ir::AbiParam;
    use cranelift_module::Linkage;

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
                sig.params.push(AbiParam::new(RocaType::from(&param.type_ref).to_cranelift()));
            }
            sig.returns.push(AbiParam::new(RocaType::from(&f.return_type).to_cranelift()));
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
    for (params, closure_body) in closures {
        let name = format!("__closure_{}_{}", params.len(), super::expr::closure_hash(&params, &closure_body));
        if compiled.funcs.contains_key(&name) { continue; }

        let mut func = Function::new(&name);
        for p in &params {
            func = func.param(p, RocaType::Number);
        }
        func = func.returns(RocaType::Number)
            .with_return_kinds(func_return_kinds.clone());

        func.build(module, rt, compiled, |body| {
            let val = emit_expr(body, &closure_body);
            emit_scope_cleanup(body.ir, &body.ctx, None);
            body.ir.ret(val);
            body.returned = true;
        })?;
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

        let func = Function::new(&name)
            .returns(RocaType::Number)
            .with_return_kinds(func_return_kinds.clone());

        func.build(module, rt, compiled, |body| {
            let val = emit_expr(body, &expr);
            emit_scope_cleanup(body.ir, &body.ctx, None);
            body.ir.ret(val);
            body.returned = true;
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
    let mut f = Function::new(&func.name);
    for p in &func.params {
        f = f.param_with_constraints(&p.name, RocaType::from(&p.type_ref), p.constraints.clone());
    }
    f = f.returns(RocaType::from(&func.return_type));
    if func.returns_err { f = f.returns_err(); }
    if let Some(crash) = &func.crash {
        for h in &crash.handlers {
            f = f.crash(&h.call, &h.strategy);
        }
    }
    f = f.with_return_kinds(func_return_kinds.clone())
        .with_enum_variants(enum_variants.clone())
        .with_struct_defs(struct_defs.clone());

    f.build(module, rt, compiled, |body| {
        // Validate constrained params at function entry
        emit_param_constraints(body, &func.params);

        for stmt in &func.body {
            if body.has_returned() { break; }
            emit_stmt(body, stmt);
        }
    })
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
    let field_info: Vec<(String, RocaType)> = fields.iter().map(|f| {
        (f.name.clone(), RocaType::from(&f.type_ref))
    }).collect();

    let mut f = Function::new(&format!("{}.{}", struct_name, func.name));
    f = f.self_param();
    for p in &func.params {
        f = f.param_with_constraints(&p.name, RocaType::from(&p.type_ref), p.constraints.clone());
    }
    f = f.returns(RocaType::from(&func.return_type));
    if func.returns_err { f = f.returns_err(); }
    if let Some(crash) = &func.crash {
        for h in &crash.handlers {
            f = f.crash(&h.call, &h.strategy);
        }
    }
    f = f.with_struct_layout(struct_name, StructLayout { fields: field_info })
        .with_self_struct_type(struct_name)
        .with_return_kinds(func_return_kinds.clone())
        .with_enum_variants(enum_variants.clone())
        .with_struct_defs(struct_defs.clone());

    f.build(module, rt, compiled, |body| {
        // Validate constrained params at method entry
        emit_param_constraints(body, &func.params);

        for stmt in &func.body {
            if body.has_returned() { break; }
            emit_stmt(body, stmt);
        }
    })
}

/// Compile an auto-stub for an extern fn — returns a default value.
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
    f = f.returns(RocaType::from(&extern_fn.return_type));
    if extern_fn.returns_err { f = f.returns_err(); }

    f.build(module, rt, compiled, |body| {
        let val = emit_expr(body, default_value_expr);
        if body.ctx.returns_err {
            let no_err = body.ir.const_bool(false);
            body.ir.ret_with_err(val, no_err);
        } else {
            body.ir.ret(val);
        }
        body.returned = true;
    })
}

/// Compile auto-stubs for all methods in an extern contract.
/// Each method gets a JIT function named "Contract.method" that returns a default value.
pub fn compile_contract_stubs<M: Module>(
    module: &mut M,
    contract: &roca::ContractDef,
    rt: &RuntimeFuncs,
    compiled: &mut CompiledFuncs,
) -> Result<(), String> {
    for sig_def in &contract.functions {
        let qualified = format!("{}.{}", contract.name, sig_def.name);
        if compiled.funcs.contains_key(&qualified) { continue; }

        let mut f = Function::new(&qualified);
        for p in &sig_def.params {
            f = f.param(&p.name, RocaType::from(&p.type_ref));
        }
        f = f.returns(RocaType::from(&sig_def.return_type));
        if sig_def.returns_err { f = f.returns_err(); }

        let result = f.build(module, rt, compiled, |body| {
            let ret_val = body.ir.default_for(&RocaType::from(&sig_def.return_type));
            if body.ctx.returns_err {
                let no_err = body.ir.const_bool(false);
                body.ir.ret_with_err(ret_val, no_err);
            } else {
                body.ir.ret(ret_val);
            }
            body.returned = true;
        });

        if result.is_err() {
            // Skip stubs that fail to compile (e.g., generic params)
            continue;
        }
    }
    Ok(())
}
