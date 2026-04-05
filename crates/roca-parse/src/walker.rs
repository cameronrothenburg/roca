//! AST walker — owns the state table and calls rules at each check point.
//!
//! Rules are pure observers. The walker owns all state mutations:
//!   - const → Owned, let → Borrowed, var → Owned
//!   - passing to `o` param → Consumed
//!   - branch / loop cloning and reconciliation

use std::collections::{HashMap, HashSet};

use roca_lang::ast::{Expr, ExprKind, FuncDef, Item, Own, Param, SourceFile, Stmt, StructDef, Type};

use crate::rule::{
    build_registries, infer_expr_type, type_to_name, Ctx, Diagnostic, FieldRegistry, FnRegistry,
    FnSigRegistry, Rule, StateTable, TypeRegistry, VarInfo, VarState,
};

// ─── State table helpers ──────────────────────────────────────────────────────

/// Check all identifiers in an expression for use-after-move (E-OWN-004).
fn check_consumed_in_expr(expr: &Expr, state: &StateTable, diags: &mut Vec<Diagnostic>) {
    match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(info) = state.get(name) {
                if info.state == VarState::Consumed {
                    diags.push(Diagnostic {
                        code: "E-OWN-004",
                        message: format!("use of moved value '{name}'"),
                    });
                }
            }
        }
        ExprKind::BinOp { left, right, .. } => {
            check_consumed_in_expr(left, state, diags);
            check_consumed_in_expr(right, state, diags);
        }
        ExprKind::UnaryOp { expr: inner, .. } => {
            check_consumed_in_expr(inner, state, diags);
        }
        ExprKind::Call { target, args } => {
            check_consumed_in_expr(target, state, diags);
            for a in args { check_consumed_in_expr(a, state, diags); }
        }
        ExprKind::GetField { target, .. } => {
            check_consumed_in_expr(target, state, diags);
        }
        ExprKind::ArrayGet { target, index } => {
            check_consumed_in_expr(target, state, diags);
            check_consumed_in_expr(index, state, diags);
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, v) in fields { check_consumed_in_expr(v, state, diags); }
        }
        ExprKind::ArrayNew(elems) => {
            for e in elems { check_consumed_in_expr(e, state, diags); }
        }
        _ => {}
    }
}

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
    match &target.kind {
        ExprKind::Ident(n) => Some(n.clone()),
        ExprKind::GetField { target: obj, field } => match &obj.as_ref().kind {
            ExprKind::Ident(struct_name) => {
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
    match &expr.kind {
        ExprKind::GetField { target, field } => {
            check_field_access_expr(target, state, field_reg, diags);
            if let ExprKind::Ident(name) = &target.as_ref().kind {
                if let Some(info) = state.get(name) {
                    if let Some(ty_name) = &info.ty {
                        if let Some(fields) = field_reg.get(ty_name) {
                            if !fields.iter().any(|(n, _)| n == field) {
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
        ExprKind::Call { target, args } => {
            // target may be GetField (method call) — skip it, methods aren't fields
            for a in args {
                check_field_access_expr(a, state, field_reg, diags);
            }
        }
        ExprKind::BinOp { left, right, .. } => {
            check_field_access_expr(left, state, field_reg, diags);
            check_field_access_expr(right, state, field_reg, diags);
        }
        ExprKind::StructLit { fields, .. } => {
            for (_, v) in fields {
                check_field_access_expr(v, state, field_reg, diags);
            }
        }
        ExprKind::ArrayNew(elems) => {
            for e in elems {
                check_field_access_expr(e, state, field_reg, diags);
            }
        }
        ExprKind::UnaryOp { expr, .. } | ExprKind::Cast { expr, .. } | ExprKind::Wait(expr) => {
            check_field_access_expr(expr, state, field_reg, diags);
        }
        ExprKind::If { cond, then, else_ } => {
            check_field_access_expr(cond, state, field_reg, diags);
            check_field_access_expr(then, state, field_reg, diags);
            if let Some(e) = else_ {
                check_field_access_expr(e, state, field_reg, diags);
            }
        }
        _ => {}
    }
}

// ─── Registries bundle ────────────────────────────────────────────────────────

struct Regs<'a> {
    fn_reg: &'a FnRegistry,
    sig_reg: &'a FnSigRegistry,
    type_reg: &'a TypeRegistry,
    field_reg: &'a FieldRegistry,
}

impl<'a> Regs<'a> {
    fn ctx<'b>(&'b self, state: &'b StateTable) -> Ctx<'b> {
        Ctx { state, fn_reg: self.fn_reg, sig_reg: self.sig_reg, type_reg: self.type_reg, field_reg: self.field_reg }
    }

    fn infer_type(&self, expr: &Expr, state: &StateTable) -> Option<String> {
        infer_expr_type(expr, state, self.sig_reg, self.field_reg)
    }
}

// ─── Walker ───────────────────────────────────────────────────────────────────

fn walk_function(
    f: &FuncDef,
    regs: &Regs,
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
        let ctx = regs.ctx(&state);
        for rule in rules.iter_mut() {
            diags.extend(rule.check_param(p, &ctx));
        }
    }

    // Walk the body
    walk_stmts(&f.body, &f.ret, regs, rules, &mut state, diags);

    // Second pass: field access checking (read-only, needs final state)
    check_field_accesses_in_stmts(&f.body, &state, regs.field_reg, diags);
}

/// Walk a slice of statements, mutating `state` as bindings are introduced.
fn walk_stmts(
    stmts: &[Stmt],
    ret_ty: &Type,
    regs: &Regs,
    rules: &mut Vec<Box<dyn Rule>>,
    state: &mut StateTable,
    diags: &mut Vec<Diagnostic>,
) {
    for stmt in stmts {
        walk_stmt(stmt, ret_ty, regs, rules, state, diags);
    }
}

fn walk_stmt(
    stmt: &Stmt,
    ret_ty: &Type,
    regs: &Regs,
    rules: &mut Vec<Box<dyn Rule>>,
    state: &mut StateTable,
    diags: &mut Vec<Diagnostic>,
) {
    // Let rules observe the statement (pre-mutation state)
    {
        let ctx = regs.ctx(state);
        for rule in rules.iter_mut() {
            diags.extend(rule.check_stmt(stmt, &ctx));
        }
    }

    match stmt {
        // ── const x = expr → Owned ────────────────────────────────────────────
        Stmt::Let { name, value, is_const: true, .. } => {
            // Check for use-after-move in the RHS expression
            check_consumed_in_expr(value, state, diags);
            // Process any call in RHS to track moves
            if let ExprKind::Call { target, args } = &value.kind {
                process_call(target, args, regs, rules, state, diags);
            }
            let ty = regs.infer_type(value, state);
            state.insert(name.clone(), VarInfo { state: VarState::Owned, ty });
        }

        // ── let x = expr → Borrowed ───────────────────────────────────────────
        Stmt::Let { name, value, is_const: false, .. } => {
            let ty = regs.infer_type(value, state);
            state.insert(name.clone(), VarInfo { state: VarState::Borrowed, ty });
        }

        // ── var x = expr → Owned (mutable) ───────────────────────────────────
        Stmt::Var { name, value, .. } => {
            let ty = regs.infer_type(value, state);
            state.insert(name.clone(), VarInfo { state: VarState::Owned, ty });
        }

        // ── Assign: reassign a var ────────────────────────────────────────────
        Stmt::Assign { target, value } => {
            let ty = regs.infer_type(value, state);
            state.insert(target.clone(), VarInfo { state: VarState::Owned, ty });
        }

        // ── Expr statement ────────────────────────────────────────────────────
        Stmt::Expr(expr) => {
            if let ExprKind::Call { target, args } = &expr.kind {
                process_call(target, args, regs, rules, state, diags);
            }
        }

        // ── Return ────────────────────────────────────────────────────────────
        Stmt::Return(expr) => {
            check_consumed_in_expr(expr, state, diags);
            if let ExprKind::Call { target, args } = &expr.kind {
                process_call(target, args, regs, rules, state, diags);
            }
            let ctx = regs.ctx(state);
            for rule in rules.iter_mut() {
                diags.extend(rule.check_return(expr, ret_ty, &ctx));
            }
        }

        // ── If statement ──────────────────────────────────────────────────────
        Stmt::If { cond: _, then, else_ } => {
            let mut then_state = state.clone();
            walk_stmts(then, ret_ty, regs, rules, &mut then_state, diags);
            let consumed_in_then = vars_newly_consumed(state, &then_state);

            let consumed_in_else: Option<HashSet<String>> = if let Some(else_stmts) = else_ {
                let mut else_state = state.clone();
                walk_stmts(else_stmts, ret_ty, regs, rules, &mut else_state, diags);
                Some(vars_newly_consumed(state, &else_state))
            } else {
                None
            };

            {
                let ctx = regs.ctx(state);
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
            walk_stmts(body, ret_ty, regs, rules, &mut loop_state, diags);

            {
                let ctx = regs.ctx(state);
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
            walk_stmts(body, ret_ty, regs, rules, &mut for_state, diags);

            {
                let ctx = regs.ctx(state);
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
    regs: &Regs,
    rules: &mut Vec<Box<dyn Rule>>,
    state: &mut StateTable,
    diags: &mut Vec<Diagnostic>,
) {
    let fn_name = resolve_call_name(target, state);
    let qualifiers: Vec<Option<Own>> = fn_name
        .as_ref()
        .and_then(|n| regs.fn_reg.get(n))
        .cloned()
        .unwrap_or_default();

    // Call rules for each arg (pre-mutation state)
    for (i, arg) in args.iter().enumerate() {
        let qual = qualifiers.get(i).copied().flatten();
        let ctx = regs.ctx(state);
        for rule in rules.iter_mut() {
            diags.extend(rule.check_call_arg(arg, qual, &ctx));
        }
    }

    // Apply consumption: vars passed to `o` params become Consumed
    for (i, arg) in args.iter().enumerate() {
        let qual = qualifiers.get(i).copied().flatten();
        if qual == Some(Own::O) {
            if let ExprKind::Ident(name) = &arg.kind {
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
    let (fn_reg, sig_reg, type_reg, field_reg) = build_registries(source);
    let regs = Regs { fn_reg: &fn_reg, sig_reg: &sig_reg, type_reg: &type_reg, field_reg: &field_reg };
    let mut diags = Vec::new();

    for item in &source.items {
        match item {
            Item::Function(f) => {
                walk_function(f, &regs, rules, &mut diags);
            }
            Item::Struct(s) => {
                walk_struct(s, &regs, rules, &mut diags);
            }
            Item::Enum(_) | Item::Import { .. } => {}
        }
    }

    diags
}

fn walk_struct(
    s: &StructDef,
    regs: &Regs,
    rules: &mut Vec<Box<dyn Rule>>,
    diags: &mut Vec<Diagnostic>,
) {
    for method in &s.methods {
        walk_function(method, regs, rules, diags);
    }
}
