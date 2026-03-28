use std::collections::HashMap;
use crate::ast::*;
use crate::errors::RuleError;

/// Validate const/let variable rules
pub fn check_variables(file: &SourceFile) -> Vec<RuleError> {
    let mut errors = Vec::new();

    for item in &file.items {
        match item {
            Item::Function(f) => check_fn_vars(f, &mut errors),
            Item::Struct(s) => {
                for m in &s.methods {
                    check_fn_vars(m, &mut errors);
                }
            }
            Item::Satisfies(sat) => {
                for m in &sat.methods {
                    check_fn_vars(m, &mut errors);
                }
            }
            _ => {}
        }
    }

    errors
}

fn check_fn_vars(f: &FnDef, errors: &mut Vec<RuleError>) {
    // Track which variables are const
    let mut consts: HashMap<String, bool> = HashMap::new();

    for stmt in &f.body {
        check_stmt_vars(stmt, &mut consts, errors);
    }
}

fn check_stmt_vars(stmt: &Stmt, consts: &mut HashMap<String, bool>, errors: &mut Vec<RuleError>) {
    match stmt {
        Stmt::Const { name, .. } => {
            consts.insert(name.clone(), true);
        }
        Stmt::Let { name, .. } | Stmt::LetResult { name, .. } => {
            consts.insert(name.clone(), false);
        }
        Stmt::Assign { name, .. } => {
            if let Some(true) = consts.get(name) {
                errors.push(RuleError {
                    code: "const-reassign".into(),
                    message: format!("cannot reassign const '{}'", name),
                    context: None,
                });
            }
        }
        Stmt::If { then_body, else_body, .. } => {
            for s in then_body {
                check_stmt_vars(s, consts, errors);
            }
            if let Some(body) = else_body {
                for s in body {
                    check_stmt_vars(s, consts, errors);
                }
            }
        }
        Stmt::For { body, .. } => {
            for s in body {
                check_stmt_vars(s, consts, errors);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn const_reassign_fails() {
        let file = parse::parse(r#"
            fn bad() -> Number {
                const x = 5
                x = 10
                return x
                test { self() == 5 }
            }
        "#);
        let errors = check_variables(&file);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].code, "const-reassign");
    }

    #[test]
    fn let_reassign_ok() {
        let file = parse::parse(r#"
            fn ok() -> Number {
                let x = 5
                x = 10
                return x
                test { self() == 10 }
            }
        "#);
        let errors = check_variables(&file);
        assert!(errors.is_empty());
    }
}
