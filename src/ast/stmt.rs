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
    /// return err.name
    ReturnErr(String),
    Assign {
        name: String,
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
