//! All 12 checker rule implementations.
//!
//! Each struct implements [`Rule`], overriding only the hooks it needs.
//!
//! # Ownership rules
//!   E-OWN-001  ConstOwns          — orphan value in expr statement
//!   E-OWN-002  LetBorrowsFromConst — let creates new value
//!   E-OWN-003  BorrowBeforePass   — const passed directly to b param
//!   E-OWN-004  UseAfterMove       — use after move
//!   E-OWN-005  DeclareIntent      — param missing o/b
//!   E-OWN-006  ReturnOwned        — return borrowed struct
//!   E-OWN-007  ContainerCopy      — borrowed in container (note)
//!   E-OWN-009  BranchSymmetry     — asymmetric branch consumption
//!   E-OWN-010  LoopConsumption    — consume in loop without reassign
//!
//! # Type rules
//!   E-TYP-001  ReturnTypeMismatch
//!   E-TYP-002  UnknownType
//!   E-STR-006  UnknownField

use std::collections::HashSet;

use roca_lang::ast::{BinOp, Expr, ExprKind, Own, Param, Stmt, Type};

use crate::rule::{
    infer_expr_type, is_primitive_type, is_value_creating, type_to_name, Ctx, Diagnostic, Rule,
    StateTable, VarState,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn diag(code: &'static str, message: impl Into<String>) -> Diagnostic {
    Diagnostic { code, message: message.into() }
}

fn check_for_consumed(expr: &Expr, state: &StateTable) -> Vec<Diagnostic> {
    let mut out = vec![];
    match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(info) = state.get(name) {
                if info.state == VarState::Consumed {
                    out.push(diag("E-OWN-004", format!("use of consumed value '{}'", name)));
                }
            }
        }
        ExprKind::GetField { target, .. } => out.extend(check_for_consumed(target, state)),
        ExprKind::BinOp { left, right, .. } => {
            out.extend(check_for_consumed(left, state));
            out.extend(check_for_consumed(right, state));
        }
        ExprKind::Call { target, args } => {
            out.extend(check_for_consumed(target, state));
            for a in args {
                out.extend(check_for_consumed(a, state));
            }
        }
        ExprKind::ArrayNew(elems) => {
            for e in elems {
                out.extend(check_for_consumed(e, state));
            }
        }
        _ => {}
    }
    out
}

// ─── E-OWN-001: ConstOwns ────────────────────────────────────────────────────
//
// A value-creating expression used as a statement is an orphan owned value.

pub struct ConstOwns;

impl Rule for ConstOwns {
    fn code(&self) -> &'static str { "E-OWN-001" }

    fn check_stmt(&mut self, stmt: &Stmt, _ctx: &Ctx) -> Vec<Diagnostic> {
        match stmt {
            Stmt::Expr(expr) => {
                if !matches!(&expr.kind, ExprKind::Call { .. }) && is_value_creating(expr) {
                    vec![diag("E-OWN-001", "value-creating expression used as statement (orphan value)")]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }
}

// ─── E-OWN-002: LetBorrowsFromConst ──────────────────────────────────────────
//
// `let` cannot create a new value — it must derive from an existing const.

pub struct LetBorrowsFromConst;

impl Rule for LetBorrowsFromConst {
    fn code(&self) -> &'static str { "E-OWN-002" }

    fn check_stmt(&mut self, stmt: &Stmt, _ctx: &Ctx) -> Vec<Diagnostic> {
        match stmt {
            Stmt::Let { name, value, is_const: false, .. } => {
                if is_value_creating(value) {
                    vec![diag(
                        "E-OWN-002",
                        format!("'let {}' creates a new value; use 'const' for owned values", name),
                    )]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }
}

// ─── E-OWN-003: BorrowBeforePass ─────────────────────────────────────────────
//
// A const (Owned) variable must be borrowed via `let` before passing to a `b` param.

pub struct BorrowBeforePass;

impl Rule for BorrowBeforePass {
    fn code(&self) -> &'static str { "E-OWN-003" }

    fn check_call_arg(&mut self, arg: &Expr, qualifier: Option<Own>, ctx: &Ctx) -> Vec<Diagnostic> {
        if qualifier != Some(Own::B) {
            return vec![];
        }
        if let ExprKind::Ident(name) = &arg.kind {
            if let Some(info) = ctx.state.get(name) {
                if info.state == VarState::Owned {
                    return vec![diag(
                        "E-OWN-003",
                        format!(
                            "const '{}' passed directly to 'b' parameter; use 'let' to borrow first",
                            name
                        ),
                    )];
                }
            }
        }
        vec![]
    }
}

// ─── E-OWN-004: UseAfterMove ─────────────────────────────────────────────────
//
// Using a value after it has been consumed (passed to an `o` param).

pub struct UseAfterMove;

impl Rule for UseAfterMove {
    fn code(&self) -> &'static str { "E-OWN-004" }

    fn check_call_arg(&mut self, arg: &Expr, _qualifier: Option<Own>, ctx: &Ctx) -> Vec<Diagnostic> {
        if let ExprKind::Ident(name) = &arg.kind {
            if let Some(info) = ctx.state.get(name) {
                if info.state == VarState::Consumed {
                    return vec![diag("E-OWN-004", format!("use of consumed value '{}'", name))];
                }
            }
        }
        vec![]
    }

    fn check_return(&mut self, expr: &Expr, _ret_ty: &Type, ctx: &Ctx) -> Vec<Diagnostic> {
        // Only check direct ident / field access — call args are already checked via
        // check_call_arg. The walker processes calls (and marks consumed) before invoking
        // check_return, so we must NOT re-examine call arguments here.
        match &expr.kind {
            ExprKind::Ident(name) => {
                if let Some(info) = ctx.state.get(name) {
                    if info.state == VarState::Consumed {
                        return vec![diag("E-OWN-004", format!("use of consumed value '{}'", name))];
                    }
                }
                vec![]
            }
            ExprKind::GetField { target, .. } => {
                if let ExprKind::Ident(name) = &target.as_ref().kind {
                    if let Some(info) = ctx.state.get(name) {
                        if info.state == VarState::Consumed {
                            return vec![diag("E-OWN-004", format!("use of consumed value '{}'", name))];
                        }
                    }
                }
                vec![]
            }
            _ => vec![],
        }
    }
}

// ─── E-OWN-005: DeclareIntent ────────────────────────────────────────────────
//
// Every parameter must declare `o` or `b`.

pub struct DeclareIntent;

impl Rule for DeclareIntent {
    fn code(&self) -> &'static str { "E-OWN-005" }

    fn check_param(&mut self, param: &Param, _ctx: &Ctx) -> Vec<Diagnostic> {
        if param.own.is_none() {
            vec![diag("E-OWN-005", format!("param '{}' must declare 'o' or 'b'", param.name))]
        } else {
            vec![]
        }
    }
}

// ─── E-OWN-006: ReturnOwned ──────────────────────────────────────────────────
//
// Cannot return a borrowed non-primitive value; must copy it first.
// Also fires when returning a field of a borrowed value.

pub struct ReturnOwned;

impl Rule for ReturnOwned {
    fn code(&self) -> &'static str { "E-OWN-006" }

    fn check_return(&mut self, expr: &Expr, _ret_ty: &Type, ctx: &Ctx) -> Vec<Diagnostic> {
        check_return_ownership_006(expr, ctx.state)
    }
}

fn check_return_ownership_006(expr: &Expr, state: &StateTable) -> Vec<Diagnostic> {
    match &expr.kind {
        ExprKind::Ident(name) => {
            if let Some(info) = state.get(name) {
                if info.state == VarState::Borrowed {
                    if let Some(ty_name) = &info.ty {
                        if !is_primitive_type(ty_name) {
                            return vec![diag(
                                "E-OWN-006",
                                format!(
                                    "cannot return borrowed value '{}' of type '{}'; copy it first",
                                    name, ty_name
                                ),
                            )];
                        }
                    }
                }
            }
            vec![]
        }
        ExprKind::GetField { target, .. } => {
            if let ExprKind::Ident(name) = &target.as_ref().kind {
                if let Some(info) = state.get(name) {
                    if info.state == VarState::Borrowed {
                        return vec![diag(
                            "E-OWN-006",
                            format!("cannot return field of borrowed value '{}'", name),
                        )];
                    }
                }
            } else {
                return check_return_ownership_006(target, state);
            }
            vec![]
        }
        _ => vec![],
    }
}

// ─── E-OWN-007: ContainerCopy ────────────────────────────────────────────────
//
// Borrowed value placed into a container — a copy will be made (note, not hard error).

pub struct ContainerCopy;

impl Rule for ContainerCopy {
    fn code(&self) -> &'static str { "E-OWN-007" }

    fn check_stmt(&mut self, stmt: &Stmt, ctx: &Ctx) -> Vec<Diagnostic> {
        let value = match stmt {
            Stmt::Let { value, .. } | Stmt::Var { value, .. } => value,
            _ => return vec![],
        };
        if let ExprKind::ArrayNew(elems) = &value.kind {
            check_array_for_borrowed(elems, ctx.state)
        } else {
            vec![]
        }
    }
}

fn check_array_for_borrowed(elems: &[Expr], state: &StateTable) -> Vec<Diagnostic> {
    elems
        .iter()
        .filter_map(|elem| {
            if let ExprKind::Ident(name) = &elem.kind {
                if let Some(info) = state.get(name) {
                    if info.state == VarState::Borrowed {
                        return Some(diag(
                            "E-OWN-007",
                            format!("borrowed value '{}' in container — copy will be made", name),
                        ));
                    }
                }
            }
            None
        })
        .collect()
}

// ─── E-OWN-009: BranchSymmetry ───────────────────────────────────────────────
//
// Both branches of an if/else must consume the same set of owned variables.

pub struct BranchSymmetry;

impl Rule for BranchSymmetry {
    fn code(&self) -> &'static str { "E-OWN-009" }

    fn check_branch(
        &mut self,
        then_consumed: &HashSet<String>,
        else_consumed: &Option<HashSet<String>>,
        _ctx: &Ctx,
    ) -> Vec<Diagnostic> {
        match else_consumed {
            Some(else_set) => {
                if then_consumed != else_set {
                    vec![diag(
                        "E-OWN-009",
                        "asymmetric ownership: branches consume different sets of values",
                    )]
                } else {
                    vec![]
                }
            }
            None => {
                if !then_consumed.is_empty() {
                    vec![diag(
                        "E-OWN-009",
                        "asymmetric ownership: value consumed in then-branch but no else branch",
                    )]
                } else {
                    vec![]
                }
            }
        }
    }
}

// ─── E-OWN-010: LoopConsumption ──────────────────────────────────────────────
//
// An outer owned variable consumed inside a loop without reassignment is an error.

pub struct LoopConsumption;

impl Rule for LoopConsumption {
    fn code(&self) -> &'static str { "E-OWN-010" }

    fn check_loop_body(
        &mut self,
        outer_owned: &HashSet<String>,
        body_state: &StateTable,
        body: &[Stmt],
        _ctx: &Ctx,
    ) -> Vec<Diagnostic> {
        let mut out = vec![];
        for name in outer_owned {
            if body_state.get(name).map_or(false, |v| v.state == VarState::Consumed) {
                let reassigned = body.iter().any(|s| {
                    matches!(s, Stmt::Assign { target, .. } if target == name)
                });
                if !reassigned {
                    out.push(diag(
                        "E-OWN-010",
                        format!("value '{}' consumed inside loop without reassignment", name),
                    ));
                }
            }
        }
        out
    }
}

// ─── E-TYP-001: ReturnTypeMismatch ───────────────────────────────────────────
//
// The inferred return expression type does not match the declared return type.

pub struct ReturnTypeMismatch;

impl Rule for ReturnTypeMismatch {
    fn code(&self) -> &'static str { "E-TYP-001" }

    fn check_return(&mut self, expr: &Expr, ret_ty: &Type, ctx: &Ctx) -> Vec<Diagnostic> {
        let declared = match type_to_name(ret_ty) {
            Some(n) => n,
            None => return vec![],
        };
        let actual = match infer_expr_type(expr, ctx.state, ctx.sig_reg, ctx.field_reg) {
            Some(n) => n,
            None => return vec![],
        };
        if declared != actual {
            vec![diag(
                "E-TYP-001",
                format!("return type mismatch: expected {}, got {}", declared, actual),
            )]
        } else {
            vec![]
        }
    }
}

// ─── E-TYP-002: UnknownType ──────────────────────────────────────────────────
//
// A parameter uses a type name that is not registered (not a builtin or defined struct/enum).

pub struct UnknownType;

impl Rule for UnknownType {
    fn code(&self) -> &'static str { "E-TYP-002" }

    fn check_param(&mut self, param: &Param, ctx: &Ctx) -> Vec<Diagnostic> {
        if let Some(ty_name) = type_to_name(&param.ty) {
            if ty_name != "Array" && !ctx.type_reg.contains(&ty_name) {
                return vec![diag("E-TYP-002", format!("unknown type '{}'", ty_name))];
            }
        }
        vec![]
    }
}

// ─── E-STR-006: UnknownField ─────────────────────────────────────────────────
//
// Accessing a field that does not exist on the struct type.
// Note: this rule does nothing — UnknownField detection is handled by the walker's
// second-pass `check_field_accesses_in_stmts` which emits directly to diags.
// The rule struct is kept for registration completeness.

pub struct UnknownField;

impl Rule for UnknownField {
    fn code(&self) -> &'static str { "E-STR-006" }
    // No hooks needed — walker's second pass handles E-STR-006 directly.
}

// ─── E-TYP-001 (BinOp): operand type mismatch in binary operations ──────────

pub struct BinOpTypeMismatch;

impl Rule for BinOpTypeMismatch {
    fn code(&self) -> &'static str { "E-TYP-001" }

    fn check_stmt(&mut self, stmt: &Stmt, ctx: &Ctx) -> Vec<Diagnostic> {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Var { value, .. } => check_binop_types(value, ctx),
            Stmt::Return(expr) => check_binop_types(expr, ctx),
            Stmt::Assign { value, .. } => check_binop_types(value, ctx),
            _ => vec![],
        }
    }
}

fn check_binop_types(expr: &Expr, ctx: &Ctx) -> Vec<Diagnostic> {
    if let ExprKind::BinOp { op, left, right } = &expr.kind {
        // Comparisons (==, !=, <, >, etc.) only need both sides to be the same type
        // Arithmetic (+, -, *, /, %) needs both sides to be the same numeric type
        let lt = infer_expr_type(left, ctx.state, ctx.sig_reg, ctx.field_reg);
        let rt = infer_expr_type(right, ctx.state, ctx.sig_reg, ctx.field_reg);
        if let (Some(l), Some(r)) = (&lt, &rt) {
            if l != r {
                return vec![diag("E-TYP-001", format!("type mismatch in binary op: {} vs {}", l, r))];
            }
        }
        // Recurse into sub-expressions
        let mut out = check_binop_types(left, ctx);
        out.extend(check_binop_types(right, ctx));
        return out;
    }
    vec![]
}

// ─── E-TYP-001 (Call): argument type mismatch in function calls ─────────────

pub struct CallArgTypeMismatch;

impl Rule for CallArgTypeMismatch {
    fn code(&self) -> &'static str { "E-TYP-001" }

    fn check_stmt(&mut self, stmt: &Stmt, ctx: &Ctx) -> Vec<Diagnostic> {
        match stmt {
            Stmt::Let { value, .. } | Stmt::Var { value, .. } => check_call_arg_types(value, ctx),
            Stmt::Return(expr) => check_call_arg_types(expr, ctx),
            _ => vec![],
        }
    }
}

fn check_call_arg_types(expr: &Expr, ctx: &Ctx) -> Vec<Diagnostic> {
    if let ExprKind::Call { target, args } = &expr.kind {
        let fn_name = match &target.kind {
            ExprKind::Ident(n) => Some(n.clone()),
            ExprKind::GetField { target: obj, field } => {
                if let ExprKind::Ident(sn) = &obj.kind {
                    Some(format!("{}.{}", sn, field))
                } else { None }
            }
            _ => None,
        };
        if let Some(name) = fn_name {
            if let Some((param_types, _)) = ctx.sig_reg.get(&name) {
                for (i, arg) in args.iter().enumerate() {
                    if let Some(expected_ty) = param_types.get(i) {
                        let expected_name = match type_to_name(expected_ty) {
                            Some(n) => n,
                            None => continue,
                        };
                        let actual_name = match infer_expr_type(arg, ctx.state, ctx.sig_reg, ctx.field_reg) {
                            Some(n) => n,
                            None => continue,
                        };
                        if expected_name != actual_name {
                            return vec![diag("E-TYP-001",
                                format!("argument {} type mismatch: expected {}, got {}", i + 1, expected_name, actual_name))];
                        }
                    }
                }
            }
        }
    }
    vec![]
}

// ─── Registration ───────────────────────────────────────────────────────────

pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(ConstOwns),
        Box::new(LetBorrowsFromConst),
        Box::new(BorrowBeforePass),
        Box::new(UseAfterMove),
        Box::new(DeclareIntent),
        Box::new(ReturnOwned),
        Box::new(ContainerCopy),
        Box::new(BranchSymmetry),
        Box::new(LoopConsumption),
        Box::new(ReturnTypeMismatch),
        Box::new(UnknownType),
        Box::new(UnknownField),
        Box::new(BinOpTypeMismatch),
        Box::new(CallArgTypeMismatch),
    ]
}
