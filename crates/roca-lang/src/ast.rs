//! Shared IR — the single representation both backends read.
//!
//! ~26 node types at source-language semantic level. Structured control flow,
//! abstract types, no memory management operations. Ownership is tracked by
//! the checker (const = own, let = borrow, o/b on params). Memory operations
//! (alloc, drop, RC) are injected during backend-specific lowering — not here.

// ─── Literals ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Unit,
}

// ─── Types ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Float,
    String,
    Bool,
    Unit,
    Named(String),
    Array(Box<Type>),
    Fn(Vec<Type>, Box<Type>),
    Optional(Box<Type>),
}

// ─── Ownership ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Own {
    O,  // owned — caller transfers ownership
    B,  // borrowed — caller retains ownership
}

// ─── Parameters ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub own: Option<Own>,  // None = not declared, checker rejects (E-OWN-005)
    pub name: String,
    pub ty: Type,
}

// ─── Expressions ─────────────────────────────────────

/// Every expression carries its resolved type. The checker fills `ty` during
/// its walk. The compiler reads it — no type inference in the backend.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: Type,
}

impl Expr {
    pub fn untyped(kind: ExprKind) -> Self {
        Expr { kind, ty: Type::Unit }
    }
    pub fn typed(kind: ExprKind, ty: Type) -> Self {
        Expr { kind, ty }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    Lit(Lit),
    Ident(String),
    BinOp { op: BinOp, left: Box<Expr>, right: Box<Expr> },
    UnaryOp { op: UnaryOp, expr: Box<Expr> },
    Cast { expr: Box<Expr>, ty: Type },
    Call { target: Box<Expr>, args: Vec<Expr> },
    MakeClosure { params: Vec<String>, body: Box<Expr> },
    CallClosure { closure: Box<Expr>, args: Vec<Expr> },
    GetField { target: Box<Expr>, field: String },
    ArrayGet { target: Box<Expr>, index: Box<Expr> },
    StructLit { name: String, fields: Vec<(String, Expr)> },
    EnumVariant { name: String, variant: String, args: Vec<Expr> },
    ArrayNew(Vec<Expr>),
    If { cond: Box<Expr>, then: Box<Expr>, else_: Option<Box<Expr>> },
    Match { value: Box<Expr>, arms: Vec<MatchArm> },
    Block(Vec<Stmt>, Option<Box<Expr>>),
    Wait(Box<Expr>),
    SelfRef,
}

// ─── Statements ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    // Bindings
    Let { name: String, ty: Option<Type>, value: Expr, is_const: bool },  // is_const=true for `const`, false for `let`
    Var { name: String, ty: Option<Type>, value: Expr },       // mutable, owns
    Assign { target: String, value: Expr },                     // reassign var

    // Fields
    SetField { target: Expr, field: String, value: Expr },
    ArraySet { target: Expr, index: Expr, value: Expr },

    // Control flow
    Return(Expr),
    If { cond: Expr, then: Vec<Stmt>, else_: Option<Vec<Stmt>> },
    Loop { body: Vec<Stmt> },
    For { name: String, iter: Expr, body: Vec<Stmt> },
    Break,
    Continue,

    // Expression statement
    Expr(Expr),
}

// ─── Operators ───────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinOp {
    Add, Sub, Mul, Div, Mod,
    Eq, Ne, Lt, Gt, Le, Ge,
    And, Or,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

// ─── Pattern matching ────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Lit(Lit),
    Variant { name: String, variant: String, bindings: Vec<String> },
    Wildcard,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Expr,
}

// ─── Top-level items ─────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct FuncDef {
    pub name: String,
    pub is_pub: bool,
    pub params: Vec<Param>,
    pub ret: Type,
    pub body: Vec<Stmt>,
    pub test: Option<TestBlock>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructDef {
    pub name: String,
    pub is_pub: bool,
    pub fields: Vec<Field>,
    pub methods: Vec<FuncDef>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub is_pub: bool,
    pub variants: Vec<Variant>,
    pub doc: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Variant {
    Unit(String),
    Data(String, Vec<Type>),
}

// ─── Tests ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TestBlock {
    pub cases: Vec<TestCase>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestCase {
    Equals { args: Vec<Expr>, expected: Expr },
}

// ─── Source file ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Function(FuncDef),
    Struct(StructDef),
    Enum(EnumDef),
    Import { names: Vec<String>, path: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceFile {
    pub items: Vec<Item>,
}
