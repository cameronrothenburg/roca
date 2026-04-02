//! Semantic type model for the Roca language, shared across all compiler stages.
//!
//! Depends on [`roca_ast`] and provides `From<AST>` conversions so that
//! higher-level crates (`roca-cranelift`, `roca-native`) can work with
//! resolved types instead of raw syntax nodes.
//!
//! # Key types
//!
//! - [`RocaType`] — the core type enum (`Number`, `String`, `Array(T)`,
//!   `Struct(name)`, `Optional(T)`, `Fn(params, ret)`, etc.).
//! - [`Param`] / [`Field`] — typed parameters and struct fields with
//!   optional [`Constraint`]s (`Min`, `MaxLen`, `Pattern`, ...).
//! - [`FnSignature`] — full function signature including errors, used in
//!   contract definitions.
//! - [`CrashBlock`] / [`TestBlock`] — semantic mirrors of the AST crash
//!   and test nodes.
//!
//! # Example
//!
//! ```
//! use roca_types::RocaType;
//!
//! let ty = RocaType::Array(Box::new(RocaType::String));
//! assert!(ty.is_heap());
//! assert_eq!(ty.base_name(), "Array");
//! ```

mod convert;

use roca_ast::Expr;

// ─── Core Type ────────────────────────────────────────

/// The Roca type system.
#[derive(Debug, Clone, PartialEq)]
pub enum RocaType {
    Number,
    Bool,
    Void,
    String,
    Array(Box<RocaType>),
    Map(Box<RocaType>, Box<RocaType>),
    Struct(std::string::String),
    Enum(std::string::String),
    Optional(Box<RocaType>),
    Fn(Vec<RocaType>, Box<RocaType>),
    Unknown,
}

impl RocaType {
    pub fn is_heap(&self) -> bool {
        match self {
            RocaType::String | RocaType::Array(_) | RocaType::Map(_, _) | RocaType::Struct(_) | RocaType::Enum(_) => true,
            RocaType::Optional(inner) => inner.is_heap(),
            _ => false,
        }
    }
    pub fn is_primitive(&self) -> bool {
        matches!(self, RocaType::Number | RocaType::Bool | RocaType::Void)
    }
    pub fn is_nullable(&self) -> bool { matches!(self, RocaType::Optional(_)) }
    pub fn element_type(&self) -> Option<&RocaType> {
        match self { RocaType::Array(inner) | RocaType::Optional(inner) => Some(inner), _ => None }
    }
    pub fn base_name(&self) -> &str {
        match self {
            RocaType::Number => "Number", RocaType::Bool => "Bool", RocaType::Void => "Void",
            RocaType::String => "String", RocaType::Array(_) => "Array", RocaType::Map(_, _) => "Map",
            RocaType::Struct(n) | RocaType::Enum(n) => n,
            RocaType::Optional(_) => "Optional", RocaType::Fn(_, _) => "Fn", RocaType::Unknown => "Unknown",
        }
    }
    pub fn unwrap_optional(&self) -> &RocaType {
        match self { RocaType::Optional(inner) => inner, other => other }
    }
}

// ─── Param & Field ────────────────────────────────────

/// Function parameter with type and constraints.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: std::string::String,
    pub roca_type: RocaType,
    pub constraints: Vec<Constraint>,
}

/// Struct/contract field with type and constraints.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: std::string::String,
    pub roca_type: RocaType,
    pub constraints: Vec<Constraint>,
}

/// Value constraint for validation and property testing.
#[derive(Debug, Clone, PartialEq)]
pub enum Constraint {
    Min(f64),
    Max(f64),
    MinLen(f64),
    MaxLen(f64),
    Contains(std::string::String),
    Pattern(std::string::String),
    Default(std::string::String),
}

/// Named error declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrDecl {
    pub name: std::string::String,
    pub message: std::string::String,
}

/// Function signature (used in contracts).
#[derive(Debug, Clone, PartialEq)]
pub struct FnSignature {
    pub name: std::string::String,
    pub is_pub: bool,
    pub params: Vec<Param>,
    pub return_type: RocaType,
    pub returns_err: bool,
    pub errors: Vec<ErrDecl>,
}

// ─── Crash Block ──────────────────────────────────────

/// Crash block — error recovery strategies.
#[derive(Debug, Clone, PartialEq)]
pub struct CrashBlock {
    pub handlers: Vec<CrashHandler>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrashHandler {
    pub call: std::string::String,
    pub strategy: CrashHandlerKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CrashHandlerKind {
    Simple(CrashChain),
    Detailed { arms: Vec<CrashArm>, default: Option<CrashChain> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CrashArm {
    pub err_name: std::string::String,
    pub chain: CrashChain,
}

pub type CrashChain = Vec<CrashStep>;

#[derive(Debug, Clone, PartialEq)]
pub enum CrashStep {
    Log,
    Panic,
    Halt,
    Skip,
    Retry { attempts: u32, delay_ms: u32 },
    Fallback(Expr),
}

// ─── Test Block ───────────────────────────────────────

/// Inline test block — proof tests for a function.
#[derive(Debug, Clone, PartialEq)]
pub struct TestBlock {
    pub cases: Vec<TestCase>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestCase {
    Equals { args: Vec<Expr>, expected: Expr },
    IsOk { args: Vec<Expr> },
    IsErr { args: Vec<Expr>, err_name: std::string::String },
}

// ─── Tests ────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_are_not_heap() {
        assert!(!RocaType::Number.is_heap());
        assert!(!RocaType::Bool.is_heap());
    }

    #[test]
    fn heap_types_are_heap() {
        assert!(RocaType::String.is_heap());
        assert!(RocaType::Struct("Email".into()).is_heap());
    }

    #[test]
    fn extern_types_are_heap() {
        assert!(RocaType::Struct("Redis".into()).is_heap());
    }

}
