//! Rule: const-reassign
//! Detects reassignment of `const` bindings within function bodies.

use std::collections::HashMap;
use roca_ast::*;
use roca_errors as errors;
use roca_errors::RuleError;
use crate::rule::Rule;
use crate::context::FnCheckContext;

#[cfg(test)]
mod tests {
    

    fn errors(src: &str) -> Vec<roca_errors::RuleError> {
        crate::check(&roca_parse::parse(src))
    }

    #[test]
    fn const_reassign_caught() {
        assert!(errors(r#"fn bad() -> Number { const x = 5 x = 10 return x test { self() == 5 } }"#)
            .iter().any(|e| e.code == "const-reassign"));
    }

    #[test]
    fn let_reassign_ok() {
        assert!(!errors(r#"fn ok() -> Number { let x = 5 x = 10 return x test { self() == 10 } }"#)
            .iter().any(|e| e.code == "const-reassign"));
    }

    #[test]
    fn const_in_if_block_reassign() {
        // const declared inside if block, then assigned outside — the rule uses a flat map
        // so the const leaks out and the reassign outside IS caught
        let e = errors(r#"fn bad() -> Number { if true { const x = 5 } x = 10 return 10 test { self() == 10 } }"#);
        assert!(e.iter().any(|e| e.code == "const-reassign"));
    }

    #[test]
    fn for_loop_binding_not_const() {
        // for loop binding is not tracked as const or let, so reassigning it is not caught
        let e = errors(r#"fn ok() -> Number { let total = 0 for item in items { total = total + 1 } return total test { self() == 0 } }"#);
        assert!(!e.iter().any(|e| e.code == "const-reassign"));
    }

    #[test]
    fn multiple_lets_ok() {
        let e = errors(r#"fn ok() -> Number { let a = 1 let b = 2 a = 10 b = 20 return a + b test { self() == 30 } }"#);
        assert!(!e.iter().any(|e| e.code == "const-reassign"));
    }

    #[test]
    fn const_used_not_reassigned() {
        let e = errors(r#"fn ok() -> Number { const x = 5 const y = x + 1 return y test { self() == 6 } }"#);
        assert!(!e.iter().any(|e| e.code == "const-reassign"));
    }
}

pub struct VariablesRule;

impl Rule for VariablesRule {
    fn name(&self) -> &'static str { "variables" }

    fn check_function(&self, ctx: &FnCheckContext) -> Vec<RuleError> {
        let mut errors = Vec::new();
        let mut consts: HashMap<String, bool> = HashMap::new();
        for stmt in &ctx.func.def.body {
            check_stmt_vars(stmt, &mut consts, &mut errors);
        }
        errors
    }
}

fn check_stmt_vars(stmt: &Stmt, consts: &mut HashMap<String, bool>, errors: &mut Vec<RuleError>) {
    match stmt {
        Stmt::Const { name, .. } => { consts.insert(name.clone(), true); }
        Stmt::Let { name, .. } | Stmt::LetResult { name, .. } => { consts.insert(name.clone(), false); }
        Stmt::Assign { name, .. } => {
            if let Some(true) = consts.get(name) {
                errors.push(RuleError::new(errors::CONST_REASSIGN, format!("cannot reassign const '{}'", name), None));
            }
        }
        Stmt::If { then_body, else_body, .. } => {
            for s in then_body { check_stmt_vars(s, consts, errors); }
            if let Some(body) = else_body { for s in body { check_stmt_vars(s, consts, errors); } }
        }
        Stmt::For { body, .. } | Stmt::While { body, .. } => {
            for s in body { check_stmt_vars(s, consts, errors); }
        }
        _ => {}
    }
}
