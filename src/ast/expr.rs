#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    String(String),
    Number(f64),
    Bool(bool),
    Ident(String),
    /// Binary operation: left op right
    BinOp {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },
    /// Function/method call: name(args)
    Call {
        target: Box<Expr>,
        args: Vec<Expr>,
    },
    /// Field access: expr.field
    FieldAccess {
        target: Box<Expr>,
        field: String,
    },
    /// Struct literal: Name { field: value, ... }
    StructLit {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    /// Array literal: [1, 2, 3]
    Array(Vec<Expr>),
    /// Index access: expr[index]
    Index {
        target: Box<Expr>,
        index: Box<Expr>,
    },
    /// Match expression: match value { pattern => result, _ => default }
    Match {
        value: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// String interpolation: "hello {name}, you are {age}"
    /// Parts alternate: literal, expr, literal, expr, ...
    StringInterp(Vec<StringPart>),
    /// Logical not: !expr
    Not(Box<Expr>),
    /// wait expr — async await as expression
    Await(Box<Expr>),
    /// Closure: fn(params) -> expr
    Closure {
        params: Vec<String>,
        body: Box<Expr>,
    },
    /// null literal
    Null,
    /// self keyword
    SelfRef,
}

/// Extract the dotted call name from a call target expression.
/// e.g. `Ident("foo")` => `Some("foo")`
/// e.g. `FieldAccess { target: Ident("http"), field: "get" }` => `Some("http.get")`
/// e.g. `SelfRef.field` => `Some("self.field")`
/// Recursively handles chained field access.
pub fn expr_to_dotted_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident(name) => Some(name.clone()),
        Expr::SelfRef => Some("self".to_string()),
        Expr::FieldAccess { target, field } => {
            expr_to_dotted_name(target).map(|p| format!("{}.{}", p, field))
        }
        _ => None,
    }
}

/// Extract the dotted call name from a Call or Await(Call) expression.
/// e.g. `Call { target: Ident("foo"), .. }` => `Some("foo")`
/// e.g. `Await(Call { target: FieldAccess { .. }, .. })` => resolves the field chain
pub fn call_to_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Call { target, .. } => expr_to_dotted_name(target),
        Expr::Await(inner) => call_to_name(inner),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Literal(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// None = default/wildcard (_)
    pub pattern: Option<Expr>,
    pub value: Expr,
}
