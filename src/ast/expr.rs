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
    /// Error reference: err.name
    ErrRef(String),
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
