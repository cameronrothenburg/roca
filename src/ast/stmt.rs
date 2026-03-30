use super::expr::Expr;
use super::types::TypeRef;

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Const {
        name: String,
        type_ann: Option<TypeRef>,
        value: Expr,
    },
    Let {
        name: String,
        type_ann: Option<TypeRef>,
        value: Expr,
    },
    /// let name, err = expr (destructuring result)
    LetResult {
        name: String,
        err_name: String,
        value: Expr,
    },
    Return(Expr),
    /// return err.name or return err.name("custom message")
    ReturnErr {
        name: String,
        custom_message: Option<Expr>,
    },
    Assign {
        name: String,
        value: Expr,
    },
    /// self.field = value — struct field mutation
    FieldAssign {
        target: Expr,
        field: String,
        value: Expr,
    },
    /// Expression statement
    Expr(Expr),
    /// If/else
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_body: Option<Vec<Stmt>>,
    },
    /// While loop
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },
    Break,
    Continue,
    /// For loop
    For {
        binding: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
    /// let result, failed = wait call()
    Wait {
        names: Vec<String>,
        failed_name: String,
        kind: WaitKind,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum WaitKind {
    /// wait call() — single async
    Single(Expr),
    /// wait all { call1() call2() } — parallel, all must succeed
    All(Vec<Expr>),
    /// wait first { call1() call2() } — first to resolve wins
    First(Vec<Expr>),
}

/// Walk statements recursively and collect unique error names from `return err.name` statements.
pub fn collect_returned_error_names(stmts: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    collect_err_names_inner(stmts, &mut names);
    names
}

fn collect_err_names_inner(stmts: &[Stmt], names: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::ReturnErr { name, .. } => {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
            Stmt::If { then_body, else_body, .. } => {
                collect_err_names_inner(then_body, names);
                if let Some(body) = else_body {
                    collect_err_names_inner(body, names);
                }
            }
            Stmt::For { body, .. } | Stmt::While { body, .. } => {
                collect_err_names_inner(body, names);
            }
            _ => {}
        }
    }
}
