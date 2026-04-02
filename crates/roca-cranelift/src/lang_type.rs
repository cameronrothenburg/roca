//! LangType — the trait any language's type system implements to use this toolkit.
//!
//! The toolkit doesn't need to know about language-specific types. It only needs
//! to know: is this value a heap pointer (I64)? If yes, it gets `__free(ptr)` at
//! cleanup time. The language maps its types to IR types via this trait.

use cranelift_codegen::ir;

/// Trait that any language's type system implements to use the toolkit.
pub trait LangType: Clone + PartialEq + std::fmt::Debug + 'static {
    /// Map to a Cranelift IR type (F64, I8, I64, etc.)
    fn ir_type(&self) -> ir::Type;

    /// Whether this type is heap-allocated (I64 pointer that needs `__free`).
    fn is_heap(&self) -> bool;

    /// A sentinel "unknown/unresolved" type.
    fn unknown() -> Self;

    /// The number type (F64).
    fn number() -> Self;

    /// The boolean type (I8).
    fn boolean() -> Self;
}
