//! Rule: use-after-move, move-in-loop, must-be-const, recursive-cycle
//! Enforces Roca's ownership model: const borrows, let moves.

use std::collections::{HashSet, HashMap};
use roca_ast::*;
use roca_errors as errors;
use roca_errors::RuleError;
use crate::rule::Rule;
use crate::context::{FnCheckContext, ItemContext};

pub struct OwnershipRule;

impl Rule for OwnershipRule {
    fn name(&self) -> &'static str { "ownership" }

    fn check_function(&self, ctx: &FnCheckContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        let mut moved: HashSet<String> = HashSet::new();
        let mut lets: HashSet<String> = HashSet::new();
        let mut mutated: HashSet<String> = HashSet::new();

        let empty_outer = HashSet::new();
        check_stmts(&ctx.func.def.body, &mut moved, &mut lets, &mut mutated, false, &empty_outer, &mut errors);

        // must-be-const: let declared but never mutated or moved
        for name in &lets {
            if !mutated.contains(name) && !moved.contains(name) {
                errors.push(RuleError::new(
                    errors::MUST_BE_CONST,
                    format!("'{}' is never mutated — use const instead of let", name),
                    None,
                ));
            }
        }

        errors
    }

    fn check_item(&self, ctx: &ItemContext) -> Vec<RuleError> {
        let mut errors = Vec::new();

        if let Item::Struct(s) = ctx.item {
            // Build a map of all struct definitions for graph traversal
            let mut struct_map: HashMap<String, &[Field]> = HashMap::new();
            for item in &ctx.check.file.items {
                if let Item::Struct(def) = item {
                    struct_map.insert(def.name.clone(), &def.fields);
                }
            }

            // Check each field for direct or indirect cycles
            for field in &s.fields {
                let mut visited = HashSet::new();
                visited.insert(s.name.clone());
                if has_cycle(&field.type_ref, &struct_map, &mut visited) {
                    errors.push(RuleError::new(
                        errors::RECURSIVE_CYCLE,
                        format!("struct '{}' field '{}' creates a recursive cycle — use Optional<{}> to break it",
                            s.name, field.name, type_ref_name(&field.type_ref)),
                        Some(format!("{}.{}", s.name, field.name)),
                    ));
                }
            }
        }

        errors
    }
}

/// Walk the type graph checking for cycles. Returns true if a cycle is found.
/// `visited` tracks which struct names we've already seen on this path.
/// Optional<T>, Array<T>, and other Generic types break the cycle (heap indirection).
fn has_cycle(ty: &TypeRef, structs: &HashMap<String, &[Field]>, visited: &mut HashSet<String>) -> bool {
    match ty {
        TypeRef::Named(name) => {
            // If we've seen this struct before on this path, it's a cycle
            if visited.contains(name) { return true; }
            // If it's a known struct, walk its fields
            if let Some(fields) = structs.get(name) {
                visited.insert(name.clone());
                for field in *fields {
                    if has_cycle(&field.type_ref, structs, visited) {
                        return true;
                    }
                }
                visited.remove(name);
            }
            false
        }
        // Optional<T>, Array<T>, Generic — heap-allocated pointer breaks the cycle
        TypeRef::Nullable(_) | TypeRef::Generic(_, _) => false,
        // Primitives and function types can't create cycles
        _ => false,
    }
}

fn type_ref_name(ty: &TypeRef) -> String {
    match ty {
        TypeRef::Named(n) => n.clone(),
        TypeRef::String => "String".into(),
        TypeRef::Number => "Number".into(),
        _ => "T".into(),
    }
}

/// `outer_lets` = lets declared before the current loop. Only these trigger move-in-loop.
fn check_stmts(
    stmts: &[Stmt],
    moved: &mut HashSet<String>,
    lets: &mut HashSet<String>,
    mutated: &mut HashSet<String>,
    in_loop: bool,
    outer_lets: &HashSet<String>,
    errors: &mut Vec<RuleError>,
) {
    for stmt in stmts {
        check_stmt(stmt, moved, lets, mutated, in_loop, outer_lets, errors);
    }
}

fn check_stmt(
    stmt: &Stmt,
    moved: &mut HashSet<String>,
    lets: &mut HashSet<String>,
    mutated: &mut HashSet<String>,
    in_loop: bool,
    outer_lets: &HashSet<String>,
    errors: &mut Vec<RuleError>,
) {
    match stmt {
        Stmt::Let { name, value, .. } => {
            lets.insert(name.clone());
            if let Expr::Ident(src) = value {
                if lets.contains(src) && !moved.contains(src) {
                    if in_loop && outer_lets.contains(src) {
                        errors.push(RuleError::new(errors::MOVE_IN_LOOP, format!("'{}' would be moved on each iteration", src), None));
                    } else {
                        moved.insert(src.clone());
                    }
                } else if moved.contains(src) && lets.contains(src) {
                    errors.push(RuleError::new(errors::USE_AFTER_MOVE, format!("'{}' was moved and cannot be used", src), None));
                }
            } else {
                check_expr_for_moves(value, moved, lets, mutated, outer_lets, in_loop, errors);
            }
        }
        Stmt::Const { value, .. } => {
            check_expr_for_moves(value, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Stmt::Assign { name, value, .. } => {
            mutated.insert(name.clone());
            moved.remove(name);
            check_expr_for_moves(value, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Stmt::FieldAssign { target, value, .. } => {
            // Field assignment mutates the target (e.g., c.value = 1 mutates c)
            if let Expr::Ident(name) = target {
                mutated.insert(name.clone());
            }
            check_expr_for_moves(value, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Stmt::Return(expr) => {
            check_expr_for_moves(expr, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Stmt::Expr(expr) => {
            check_expr_for_moves(expr, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Stmt::If { condition, then_body, else_body } => {
            check_expr_for_moves(condition, moved, lets, mutated, outer_lets, in_loop, errors);
            let mut then_moved = moved.clone();
            check_stmts(then_body, &mut then_moved, lets, mutated, in_loop, outer_lets, errors);
            if let Some(body) = else_body {
                let mut else_moved = moved.clone();
                check_stmts(body, &mut else_moved, lets, mutated, in_loop, outer_lets, errors);
                for name in then_moved.intersection(&else_moved) {
                    moved.insert(name.clone());
                }
            } else {
                for name in &then_moved {
                    moved.insert(name.clone());
                }
            }
        }
        Stmt::While { condition, body, .. } => {
            check_expr_for_moves(condition, moved, lets, mutated, outer_lets, in_loop, errors);
            let loop_outer = lets.clone(); // lets before loop body = outer for this loop
            check_stmts(body, moved, lets, mutated, true, &loop_outer, errors);
            // Inner-loop lets are scoped — clear their moved state
            for name in lets.difference(&loop_outer) {
                moved.remove(name);
            }
        }
        Stmt::For { iter, body, binding, .. } => {
            check_expr_for_moves(iter, moved, lets, mutated, outer_lets, in_loop, errors);
            lets.insert(binding.clone());
            mutated.insert(binding.clone()); // loop binding is reassigned each iteration
            let loop_outer = lets.clone();
            check_stmts(body, moved, lets, mutated, true, &loop_outer, errors);
            for name in lets.difference(&loop_outer) {
                moved.remove(name);
            }
        }
        _ => {}
    }
}

/// Check expressions for let-variable usage that constitutes a move.
/// A let variable passed as a function argument = move.
/// A method call on a let variable = mutation (e.g., arr.push(x)).
/// `outer_lets` tracks lets declared before the current loop — only these trigger move-in-loop.
fn check_expr_for_moves(
    expr: &Expr,
    moved: &mut HashSet<String>,
    lets: &HashSet<String>,
    mutated: &mut HashSet<String>,
    outer_lets: &HashSet<String>,
    in_loop: bool,
    errors: &mut Vec<RuleError>,
) {
    match expr {
        Expr::Ident(name) => {
            if moved.contains(name) && lets.contains(name) {
                errors.push(RuleError::new(
                    errors::USE_AFTER_MOVE,
                    format!("'{}' was moved and cannot be used — ownership was transferred", name),
                    None,
                ));
            }
        }
        Expr::Call { target, args } => {
            // Method calls on a let variable count as mutation (e.g., arr.push(x))
            if let Expr::FieldAccess { target: obj, .. } = target.as_ref() {
                if let Expr::Ident(name) = obj.as_ref() {
                    mutated.insert(name.clone());
                }
            }
            check_expr_for_moves(target, moved, lets, mutated, outer_lets, in_loop, errors);
            for arg in args {
                if let Expr::Ident(name) = arg {
                    if lets.contains(name) {
                        if moved.contains(name) {
                            errors.push(RuleError::new(
                                errors::USE_AFTER_MOVE,
                                format!("'{}' was already moved — cannot pass to function", name),
                                None,
                            ));
                        } else if in_loop && outer_lets.contains(name) {
                            errors.push(RuleError::new(
                                errors::MOVE_IN_LOOP,
                                format!("'{}' would be moved on each iteration — move is not allowed in loops", name),
                                None,
                            ));
                        } else {
                            moved.insert(name.clone());
                        }
                    }
                } else {
                    check_expr_for_moves(arg, moved, lets, mutated, outer_lets, in_loop, errors);
                }
            }
        }
        Expr::BinOp { left, right, .. } => {
            check_expr_for_moves(left, moved, lets, mutated, outer_lets, in_loop, errors);
            check_expr_for_moves(right, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Expr::FieldAccess { target, .. } => {
            check_expr_for_moves(target, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Expr::Not(inner) | Expr::Await(inner) => {
            check_expr_for_moves(inner, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Expr::Array(elements) => {
            for e in elements { check_expr_for_moves(e, moved, lets, mutated, outer_lets, in_loop, errors); }
        }
        Expr::Index { target, index } => {
            check_expr_for_moves(target, moved, lets, mutated, outer_lets, in_loop, errors);
            check_expr_for_moves(index, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Expr::StructLit { fields, .. } => {
            for (_, val) in fields { check_expr_for_moves(val, moved, lets, mutated, outer_lets, in_loop, errors); }
        }
        Expr::Match { value, arms } => {
            check_expr_for_moves(value, moved, lets, mutated, outer_lets, in_loop, errors);
            for arm in arms {
                check_expr_for_moves(&arm.value, moved, lets, mutated, outer_lets, in_loop, errors);
            }
        }
        Expr::Closure { body, .. } => {
            check_expr_for_moves(body, moved, lets, mutated, outer_lets, in_loop, errors);
        }
        Expr::StringInterp(parts) => {
            for part in parts {
                if let StringPart::Expr(e) = part {
                    check_expr_for_moves(e, moved, lets, mutated, outer_lets, in_loop, errors);
                }
            }
        }
        Expr::EnumVariant { args, .. } => {
            for a in args { check_expr_for_moves(a, moved, lets, mutated, outer_lets, in_loop, errors); }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    

    fn errors(src: &str) -> Vec<roca_errors::RuleError> {
        crate::check(&roca_parse::parse(src))
    }

    fn has_error(src: &str, code: &str) -> bool {
        errors(src).iter().any(|e| e.code == code)
    }

    // ─── Use-after-move ───────────────────────────────

    #[test]
    fn use_after_move_basic() {
        assert!(has_error(r#"
            fn bad() -> String {
                let name = "hello"
                const result = consume(name)
                return name
            test { self() == "hello" }}
        "#, "use-after-move"));
    }

    #[test]
    fn use_after_move_reassign_to_let() {
        assert!(has_error(r#"
            fn bad() -> String {
                let a = "hello"
                let b = a
                return a
            test { self() == "hello" }}
        "#, "use-after-move"));
    }

    #[test]
    fn const_borrow_not_moved() {
        assert!(!has_error(r#"
            fn ok() -> String {
                const name = "hello"
                const result = borrow_fn(name)
                return name
            test { self() == "hello" }}
        "#, "use-after-move"));
    }

    #[test]
    fn let_moved_then_reassigned_ok() {
        // After move, reassigning gives new ownership
        assert!(!has_error(r#"
            fn ok() -> String {
                let a = "hello"
                const _ = consume(a)
                a = "world"
                return a
            test { self() == "world" }}
        "#, "use-after-move"));
    }

    #[test]
    fn double_move_caught() {
        assert!(has_error(r#"
            fn bad() -> String {
                let s = "hello"
                const a = consume(s)
                const b = consume(s)
                return a
            test { self() == "hello" }}
        "#, "use-after-move"));
    }

    // ─── Move in loop ─────────────────────────────────

    #[test]
    fn move_in_loop_caught() {
        assert!(has_error(r#"
            fn bad() -> Number {
                let data = "payload"
                let i = 0
                while i < 3 {
                    const _ = consume(data)
                    i = i + 1
                }
                return 0
            test { self() == 0 }}
        "#, "move-in-loop"));
    }

    #[test]
    fn fresh_let_in_loop_ok() {
        // let declared INSIDE loop is fresh each iteration
        assert!(!has_error(r#"
            fn ok() -> Number {
                let i = 0
                while i < 3 {
                    let data = "fresh"
                    const _ = consume(data)
                    i = i + 1
                }
                return 0
            test { self() == 0 }}
        "#, "move-in-loop"));
    }

    // ─── Must be const ────────────────────────────────

    #[test]
    fn must_be_const_caught() {
        assert!(has_error(r#"
            fn bad() -> Number {
                let x = 5
                return x
            test { self() == 5 }}
        "#, "must-be-const"));
    }

    #[test]
    fn let_mutated_ok() {
        assert!(!has_error(r#"
            fn ok() -> Number {
                let x = 5
                x = 10
                return x
            test { self() == 10 }}
        "#, "must-be-const"));
    }

    #[test]
    fn let_moved_ok() {
        // Moving counts as "using mutably"
        assert!(!has_error(r#"
            fn ok() -> String {
                let s = "hello"
                return consume(s)
            test { self() == "hello" }}
        "#, "must-be-const"));
    }

    // ─── Recursive cycle ──────────────────────────────

    #[test]
    fn recursive_struct_caught() {
        assert!(has_error(r#"
            pub struct Bad {
                self_ref: Bad
            }{}
        "#, "recursive-cycle"));
    }

    #[test]
    fn optional_recursive_ok() {
        assert!(!has_error(r#"
            pub struct Node {
                value: Number
                next: Optional<Node>
            }{}
        "#, "recursive-cycle"));
    }

    #[test]
    fn array_recursive_ok() {
        assert!(!has_error(r#"
            pub struct Tree {
                children: Array<Tree>
            }{}
        "#, "recursive-cycle"));
    }

    #[test]
    fn indirect_cycle_a_b_a() {
        // A contains B, B contains A — indirect cycle
        assert!(has_error(r#"
            pub struct A { b: B }{}
            pub struct B { a: A }{}
        "#, "recursive-cycle"));
    }

    #[test]
    fn indirect_cycle_three_way() {
        // A → B → C → A
        assert!(has_error(r#"
            pub struct A { b: B }{}
            pub struct B { c: C }{}
            pub struct C { a: A }{}
        "#, "recursive-cycle"));
    }

    #[test]
    fn indirect_cycle_broken_by_optional() {
        // A contains B, B contains Optional<A> — NOT a cycle
        assert!(!has_error(r#"
            pub struct A { b: B }{}
            pub struct B { a: Optional<A> }{}
        "#, "recursive-cycle"));
    }

    #[test]
    fn primitives_not_flagged_as_cycle() {
        assert!(!has_error(r#"
            pub struct Point { x: Number y: Number }{}
        "#, "recursive-cycle"));
    }

    #[test]
    fn no_cycle_different_structs() {
        // A contains B, B contains C — no cycle (C doesn't reference A)
        assert!(!has_error(r#"
            pub struct A { b: B }{}
            pub struct B { c: C }{}
            pub struct C { value: Number }{}
        "#, "recursive-cycle"));
    }

    // ─── Adversarial: conditional move ────────────────

    #[test]
    fn move_in_one_branch_use_after() {
        assert!(has_error(r#"
            fn bad(flag: Bool) -> String {
                let data = "payload"
                if flag {
                    const _ = consume(data)
                }
                return data
            test { self(true) == "payload" }}
        "#, "use-after-move"));
    }

    #[test]
    fn move_in_both_branches_no_use_ok() {
        // Moved in both branches, but never used after — OK
        assert!(!has_error(r#"
            fn ok(flag: Bool) -> String {
                let data = "payload"
                if flag {
                    const _ = consume(data)
                } else {
                    const _ = consume(data)
                }
                return "done"
            test { self(true) == "done" }}
        "#, "use-after-move"));
    }

    // ─── Adversarial: struct field moves ──────────────

    #[test]
    fn move_struct_then_access_field() {
        assert!(has_error(r#"
            fn bad() -> String {
                let user = User { name: "cam" }
                const _ = consume(user)
                return user.name
            test { self() == "cam" }}
        "#, "use-after-move"));
    }

    // ─── Adversarial: const passed everywhere is fine ─

    #[test]
    fn const_passed_to_multiple_fns() {
        assert!(!has_error(r#"
            fn ok() -> String {
                const s = "hello"
                const a = borrow1(s)
                const b = borrow2(s)
                const c = borrow3(s)
                return s
            test { self() == "hello" }}
        "#, "use-after-move"));
    }
}
