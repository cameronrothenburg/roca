//! Rule trait, diagnostics, and shared types for ownership checking.

use std::collections::{HashMap, HashSet};

use roca_lang::ast::{Expr, ExprKind, Item, Lit, Own, Param, SourceFile, Stmt, Type};

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub code: &'static str,
    pub message: String,
}

// ─── Variable state ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum VarState {
    Owned,    // created by const, not yet consumed
    Borrowed, // b-param or let-borrow
    Consumed, // passed to an o param — dead
}

#[derive(Debug, Clone)]
pub struct VarInfo {
    pub state: VarState,
    /// Inferred type name, if known. Used for E-STR-006.
    pub ty: Option<String>,
}

// ─── Registry types ───────────────────────────────────────────────────────────

/// Maps function name → ordered list of param ownership qualifiers.
/// For struct methods, key is "StructName.method".
pub type FnRegistry = HashMap<String, Vec<Option<Own>>>;

/// Set of all known type names (builtins + struct names + enum names).
pub type TypeRegistry = HashSet<String>;

/// Maps struct name → set of field names.
pub type FieldRegistry = HashMap<String, HashSet<String>>;

/// Variable state table for a function.
pub type StateTable = HashMap<String, VarInfo>;

// ─── Type helpers ─────────────────────────────────────────────────────────────

pub fn type_to_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Int => Some("Int".into()),
        Type::Float => Some("Float".into()),
        Type::String => Some("String".into()),
        Type::Bool => Some("Bool".into()),
        Type::Unit => Some("Unit".into()),
        Type::Named(n) => Some(n.clone()),
        Type::Array(_) => Some("Array".into()),
        _ => None,
    }
}

pub fn lit_type_name(lit: &Lit) -> &'static str {
    match lit {
        Lit::Int(_) => "Int",
        Lit::Float(_) => "Float",
        Lit::String(_) => "String",
        Lit::Bool(_) => "Bool",
        Lit::Unit => "Unit",
    }
}

pub fn infer_expr_type(expr: &Expr, state: &StateTable) -> Option<String> {
    match &expr.kind {
        ExprKind::Lit(lit) => Some(lit_type_name(lit).to_string()),
        ExprKind::Ident(name) => state.get(name).and_then(|v| v.ty.clone()),
        ExprKind::StructLit { name, .. } => Some(name.clone()),
        _ => None,
    }
}

pub fn is_primitive_type(name: &str) -> bool {
    matches!(name, "Int" | "Float" | "String" | "Bool" | "Unit")
}

/// Returns true if the expression creates a brand-new value (not derived from an existing var).
pub fn is_value_creating(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::Lit(_) | ExprKind::StructLit { .. } | ExprKind::ArrayNew(_) | ExprKind::EnumVariant { .. }
    )
}

fn builtin_types() -> TypeRegistry {
    ["Int", "Float", "String", "Bool", "Unit"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Build all three registries by scanning the top-level items once.
pub fn build_registries(source: &SourceFile) -> (FnRegistry, TypeRegistry, FieldRegistry) {
    let mut fn_reg: FnRegistry = HashMap::new();
    let mut type_reg = builtin_types();
    let mut field_reg: FieldRegistry = HashMap::new();

    for item in &source.items {
        match item {
            Item::Function(f) => {
                let quals: Vec<Option<Own>> = f.params.iter().map(|p| p.own).collect();
                fn_reg.insert(f.name.clone(), quals);
            }
            Item::Struct(s) => {
                type_reg.insert(s.name.clone());
                let fields: HashSet<String> = s.fields.iter().map(|f| f.name.clone()).collect();
                field_reg.insert(s.name.clone(), fields);
                for m in &s.methods {
                    let key = format!("{}.{}", s.name, m.name);
                    let quals: Vec<Option<Own>> = m.params.iter().map(|p| p.own).collect();
                    fn_reg.insert(key, quals);
                }
            }
            Item::Enum(e) => {
                type_reg.insert(e.name.clone());
            }
            Item::Import { .. } => {}
        }
    }

    (fn_reg, type_reg, field_reg)
}

// ─── Read-only context ────────────────────────────────────────────────────────

/// Read-only context passed to rules at each check point.
pub struct Ctx<'a> {
    pub state: &'a StateTable,
    pub fn_reg: &'a FnRegistry,
    pub type_reg: &'a TypeRegistry,
    pub field_reg: &'a FieldRegistry,
}

// ─── Rule trait ───────────────────────────────────────────────────────────────

/// A single checker rule. Override only the methods your rule needs.
pub trait Rule {
    fn code(&self) -> &'static str;

    fn check_stmt(&mut self, _stmt: &Stmt, _ctx: &Ctx) -> Vec<Diagnostic> {
        vec![]
    }

    fn check_param(&mut self, _param: &Param, _ctx: &Ctx) -> Vec<Diagnostic> {
        vec![]
    }

    fn check_return(&mut self, _expr: &Expr, _ret_ty: &Type, _ctx: &Ctx) -> Vec<Diagnostic> {
        vec![]
    }

    fn check_call_arg(
        &mut self,
        _arg: &Expr,
        _qualifier: Option<Own>,
        _ctx: &Ctx,
    ) -> Vec<Diagnostic> {
        vec![]
    }

    fn check_branch(
        &mut self,
        _then_consumed: &HashSet<String>,
        _else_consumed: &Option<HashSet<String>>,
        _ctx: &Ctx,
    ) -> Vec<Diagnostic> {
        vec![]
    }

    fn check_loop_body(
        &mut self,
        _outer_owned: &HashSet<String>,
        _body_state: &StateTable,
        _body: &[Stmt],
        _ctx: &Ctx,
    ) -> Vec<Diagnostic> {
        vec![]
    }
}
