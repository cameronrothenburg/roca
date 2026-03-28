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
    /// For loop
    For {
        binding: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
}
