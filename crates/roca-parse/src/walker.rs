//! AST walker — owns the state table and calls rules at each check point.
//!
//! Rules are pure observers. The walker owns all state mutations:
//!   - const → Owned, let → Borrowed, var → Owned
//!   - passing to `o` param → Consumed
//!   - branch / loop cloning and reconciliation

use std::collections::{HashMap, HashSet};

use roca_lang::ast::{Expr, FuncDef, Item, Own, Param, SourceFile, Stmt, StructDef, Type};

use crate::rule::{
    build_registries, infer_expr_type, type_to_name, Ctx, Diagnostic, FieldRegistry, FnRegistry,
    Rule, StateTable, TypeRegistry, VarInfo, VarState,
};

// ─── State table helpers ──────────────────────────────────────────────────────

fn vars_newly_consumed(before: &StateTable, after: &StateTable) -> HashSet<String> {
    after
        .iter()
        .filter(|(k, v)| {
            v.state == VarState::Consumed
                && before.get(*k).map_or(false, |b| b.state != VarState::Consumed)
        })
        .map(|(k, _)| k.clone())
        .collect()
}

fn owned_var_names(state: &StateTable) -> HashSet<String> {
    state
        .iter()
        .filter(|(_, v)| v.state == VarState::Owned)
        .map(|(k, _)| k.clone())
        .collect()
}

/// Resolve a call target to a function name string for registry lookup.
fn resolve_call_name(target: &Expr, state: &StateTable) -> Option<String> {
    match target {
        Expr::Ident(n) => Some(n.clone()),
        Expr::GetField { target: obj, field } => match obj.as_ref() {
            Expr::Ident(struct_name) => {
                if let Some(info) = state.get(struct_name) {
                    if let Some(ty_name) = &info.ty {
                        return Some(format!("{}.{}", ty_name, field));
                    }
                }
                Some(format!("{}.{}", struct_name, field))
            }
            _ => None,
        },
        _ => None,
    }
}

// ─── Field access checker (E-STR-006) ─────────────────────────────────────────
//
// Runs as a second pass after all bindings are established (state is read-only).

fn check_field_accesses_in_stmts(
    stmts: &[Stmt],
    state: &StateTable,
    field_reg: &FieldRegistry,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        check_field_access_stmt(stmt, state, field_reg, diags);
    }
}

fn check_field_access_stmt(
    stmt: &Stmt,
    state: &StateTable,
    field_reg: &FieldRegistry,
    diags: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Return(expr) => check_field_access_expr(expr, state, field_reg, diags),
        Stmt::Let { value, .. } | Stmt::Var { value, .. } => {
            check_field_access_expr(value, state, field_reg, diags)
        }
        Stmt::Assign { value, .. } => check_field_access_expr(value, state, field_reg, diags),
        Stmt::Expr(expr) => check_field_access_expr(expr, state, field_reg, diags),
        Stmt::If { cond, then, else_ } => {
            check_field_access_expr(cond, state, field_reg, diags);
            check_field_accesses_in_stmts(then, state, field_reg, diags);
            if let Some(e) = else_ {
                check_field_accesses_in_stmts(e, state, field_reg, diags);
            }
        }
        Stmt::Loop { body } | Stmt::For { body, .. } => {
            check_field_accesses_in_stmts(body, state, field_reg, diags);
        }
        _ => {}
    }
}

fn check_field_access_expr(
    expr: &Expr,
    state: &StateTable,
    field_reg: &FieldRegistry,
    diags: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::GetField { target, field } => {
            check_field_access_expr(target, state, field_reg, diags);
            if let Expr::Ident(name) = target.as_ref() {
                if let Some(info) = state.get(name) {
                    if let Some(ty_name) = &info.ty {
                        if let Some(fields) = field_reg.get(ty_name) {
                            if !fields.contains(field) {
                                diags.push(Diagnostic {
                                    code: "E-STR-006",
                                    message: format!(
                                        "struct '{}' has no field '{}'",
                                        ty_name, field
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }
        Expr::Call { target, args } => {
            check_field_access_expr(target, state, field_reg, diags);
            for a in args {
                check_field_access_expr(a, state, field_reg, diags);
            }
        }
        Expr::BinOp { left, right, .. } => {
            check_field_access_expr(left, state, field_reg, diags);
            check_field_access_expr(right, state, field_reg, diags);
        }
        Expr::StructLit { fields, .. } => {
            for (_, v) in fields {
                check_field_access_expr(v, state, field_reg, diags);
            }
        }
        Expr::ArrayNew(elems) => {
            for e in elems {
                check_field_access_expr(e, state, field_reg, diags);
            }
        }
        Expr::UnaryOp { expr, .. } | Expr::Cast { expr, .. } | Expr::Wait(expr) => {
            check_field_access_expr(expr, state, field_reg, diags);
        }
        Expr::If { cond, then, else_ } => {
            check_field_access_expr(cond, state, field_reg, diags);
            check_field_access_expr(then, state, field_reg, diags);
            if let Some(e) = else_ {
                check_field_access_expr(e, state, field_reg, diags);
            }
        }
        _ => {}
    }
}

// ─── Walker ───────────────────────────────────────────────────────────────────

/// Walk a single function, seeding the state from its params, then walking its body.
fn walk_function(
    f: &FuncDef,
    fn_reg: &FnRegistry,
    type_reg: &TypeRegistry,
    field_reg: &FieldRegistry,
    rules: &mut Vec<Box<dyn Rule>>,
    diags: &mut Vec<Diagnostic>,
) {
    // Build initial state from params
    let mut state: StateTable = HashMap::new();
    for p in &f.params {
        let var_state = match p.own {
            Some(Own::O) => VarState::Owned,
            Some(Own::B) | None => VarState::Borrowed,
        };
        let ty = type_to_name(&p.ty);
        state.insert(p.name.clone(), VarInfo { state: var_state, ty });
    }

    // Let rules inspect each param
    for p in &f.params {
        let ctx = Ctx { state: &state, fn_reg, type_reg, field_reg };
        for rule in rules.iter_mut() {
            diags.extend(rule.check_param(p, &ctx));
        }
    }

    // Walk the body
    walk_stmts(&f.body, &f.ret, fn_reg, type_reg, field_reg, rules, &mut state, diags);

    // Second pass: field access checking (read-only, needs final state)
    check_field_accesses_in_stmts(&f.body, &state, field_reg, diags);
}

/// Walk a slice of statements, mutating `state` as bindings are introduced.
fn walk_stmts(
    stmts: &[Stmt],
    ret_ty: &Type,
    fn_reg: &FnRegistry,
    type_reg: &TypeRegistry,
    field_reg: &FieldRegistry,
    rules: &mut Vec<Box<dyn Rule>>,
    state: &mut StateTable,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        walk_stmt(stmt, ret_ty, fn_reg, type_reg, field_reg, rules, state, diags);
    }
}

fn walk_stmt(
    stmt: &Stmt,
    ret_ty: &Type,
    fn_reg: &FnRegistry,
    type_reg: &TypeRegistry,
    field_reg: &FieldRegistry,
    rules: &mut Vec<Box<dyn Rule>>,
    state: &mut StateTable,
    diags: &mut Vec<Diagnostic>,
) {
    // Let rules observe the statement (pre-mutation state)
    {
        let ctx = Ctx { state, fn_reg, type_reg, field_reg };
        for rule in rules.iter_mut() {
            diags.extend(rule.check_stmt(stmt, &ctx));
        }
    }

    match stmt {
        // ── const x = expr → Owned ────────────────────────────────────────────
        Stmt::Let { name, value, is_const: true, .. } => {
            // Process any call in RHS first to track moves
            if let Expr::Call { target, args } = value {
                process_call(target, args, fn_reg, type_reg, field_reg, rules, state, diags);
            }
            let ty = infer_expr_type(value, state);
            state.insert(name.clone(), VarInfo { state: VarState::Owned, ty });
        }

        // ── let x = expr → Borrowed ───────────────────────────────────────────
        Stmt::Let { name, value, is_const: false, .. } => {
            let ty = infer_expr_type(value, state);
            state.insert(name.clone(), VarInfo { state: VarState::Borrowed, ty });
        }

        // ── var x = expr → Owned (mutable) ───────────────────────────────────
        Stmt::Var { name, value, .. } => {
            let ty = infer_expr_type(value, state);
            state.insert(name.clone(), VarInfo { state: VarState::Owned, ty });
        }

        // ── Assign: reassign a var ────────────────────────────────────────────
        Stmt::Assign { target, value } => {
            let ty = infer_expr_type(value, state);
            state.insert(target.clone(), VarInfo { state: VarState::Owned, ty });
        }

        // ── Expr statement ────────────────────────────────────────────────────
        Stmt::Expr(expr) => {
            if let Expr::Call { target, args } = expr {
                process_call(target, args, fn_reg, type_reg, field_reg, rules, state, diags);
            }
        }

        // ── Return ────────────────────────────────────────────────────────────
        Stmt::Return(expr) => {
            // Process any call in the return expression first (for moves)
            if let Expr::Call { target, args } = expr {
                process_call(target, args, fn_reg, type_reg, field_reg, rules, state, diags);
            }
            let ctx = Ctx { state, fn_reg, type_reg, field_reg };
            for rule in rules.iter_mut() {
                diags.extend(rule.check_return(expr, ret_ty, &ctx));
            }
        }

        // ── If statement ──────────────────────────────────────────────────────
        Stmt::If { cond: _, then, else_ } => {
            let mut then_state = state.clone();
            walk_stmts(then, ret_ty, fn_reg, type_reg, field_reg, rules, &mut then_state, diags);
            let consumed_in_then = vars_newly_consumed(state, &then_state);

            let consumed_in_else: Option<HashSet<String>> = if let Some(else_stmts) = else_ {
                let mut else_state = state.clone();
                walk_stmts(
                    else_stmts,
                    ret_ty,
                    fn_reg,
                    type_reg,
                    field_reg,
                    rules,
                    &mut else_state,
                    diags,
                );
                Some(vars_newly_consumed(state, &else_state))
            } else {
                None
            };

            {
                let ctx = Ctx { state, fn_reg, type_reg, field_reg };
                for rule in rules.iter_mut() {
                    diags.extend(rule.check_branch(&consumed_in_then, &consumed_in_else, &ctx));
                }
            }

            // After if/else: vars consumed in BOTH branches are consumed in outer state
            if let Some(ref else_set) = consumed_in_else {
                for name in consumed_in_then.intersection(else_set) {
                    if let Some(info) = state.get_mut(name) {
                        info.state = VarState::Consumed;
                    }
                }
            }
        }

        // ── Loop ──────────────────────────────────────────────────────────────
        Stmt::Loop { body } => {
            let outer_owned = owned_var_names(state);
            let mut loop_state = state.clone();
            walk_stmts(body, ret_ty, fn_reg, type_reg, field_reg, rules, &mut loop_state, diags);

            {
                let ctx = Ctx { state, fn_reg, type_reg, field_reg };
                for rule in rules.iter_mut() {
                    diags.extend(rule.check_loop_body(&outer_owned, &loop_state, body, &ctx));
                }
            }

            // Propagate consumed state outward
            for (k, v) in &loop_state {
                if v.state == VarState::Consumed {
                    if let Some(outer) = state.get_mut(k) {
                        outer.state = VarState::Consumed;
                    }
                }
            }
        }

        // ── For ───────────────────────────────────────────────────────────────
        Stmt::For { name, iter: _, body } => {
            let outer_owned = owned_var_names(state);
            let mut for_state = state.clone();
            for_state.insert(name.clone(), VarInfo { state: VarState::Owned, ty: None });
            walk_stmts(body, ret_ty, fn_reg, type_reg, field_reg, rules, &mut for_state, diags);

            {
                let ctx = Ctx { state, fn_reg, type_reg, field_reg };
                for rule in rules.iter_mut() {
                    diags.extend(rule.check_loop_body(&outer_owned, &for_state, body, &ctx));
                }
            }
        }

        // ── No ownership checks ───────────────────────────────────────────────
        Stmt::SetField { .. } | Stmt::ArraySet { .. } | Stmt::Break | Stmt::Continue => {}
    }
}

/// Process a Call expression: resolve callee, call `check_call_arg` on each arg,
/// and mark consumed vars in state.
fn process_call(
    target: &Expr,
    args: &[Expr],
    fn_reg: &FnRegistry,
    type_reg: &TypeRegistry,
    field_reg: &FieldRegistry,
    rules: &mut Vec<Box<dyn Rule>>,
    state: &mut StateTable,
    diags: &mut Vec<Diagnostic>,
) {
    let fn_name = resolve_call_name(target, state);
    let qualifiers: Vec<Option<Own>> = fn_name
        .as_ref()
        .and_then(|n| fn_reg.get(n))
        .cloned()
        .unwrap_or_default();

    // Call rules for each arg (pre-mutation state)
    for (i, arg) in args.iter().enumerate() {
        let qual = qualifiers.get(i).copied().flatten();
        let ctx = Ctx { state, fn_reg, type_reg, field_reg };
        for rule in rules.iter_mut() {
            diags.extend(rule.check_call_arg(arg, qual, &ctx));
        }
    }

    // Apply consumption: vars passed to `o` params become Consumed
    for (i, arg) in args.iter().enumerate() {
        let qual = qualifiers.get(i).copied().flatten();
        if qual == Some(Own::O) {
            if let Expr::Ident(name) = arg {
                if let Some(info) = state.get_mut(name) {
                    if info.state == VarState::Owned || info.state == VarState::Borrowed {
                        info.state = VarState::Consumed;
                    }
                }
            }
        }
    }
}

// ─── Top-level entry ──────────────────────────────────────────────────────────

pub fn walk(source: &SourceFile, rules: &mut Vec<Box<dyn Rule>>) -> Vec<Diagnostic> {
    let (fn_reg, type_reg, field_reg) = build_registries(source);
    let mut diags = Vec::new();

    for item in &source.items {
        match item {
            Item::Function(f) => {
                walk_function(f, &fn_reg, &type_reg, &field_reg, rules, &mut diags);
            }
            Item::Struct(s) => {
                walk_struct(s, &fn_reg, &type_reg, &field_reg, rules, &mut diags);
            }
            Item::Enum(_) | Item::Import { .. } => {}
        }
    }

    diags
}

fn walk_struct(
    s: &StructDef,
    fn_reg: &FnRegistry,
    type_reg: &TypeRegistry,
    field_reg: &FieldRegistry,
    rules: &mut Vec<Box<dyn Rule>>,
    diags: &mut Vec<Diagnostic>,
) {
    for method in &s.methods {
        walk_function(method, fn_reg, type_reg, field_reg, rules, diags);
    }
}
