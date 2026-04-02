//! LangType — the trait any language's type system implements to use this toolkit.
//!
//! This is the bridge between a language's semantic type system and Cranelift IR.
//! Languages implement this trait for their type enum, and the toolkit handles
//! all IR generation, memory management, and cleanup automatically.

use cranelift_codegen::ir;

use crate::cranelift_type::CleanupStrategy;

/// Trait that any language's type system implements to use the toolkit.
///
/// The toolkit uses this trait to:
/// - Map language types to Cranelift IR types (F64 for floats, I8 for bools, I64 for heap pointers)
/// - Determine which values need heap cleanup on scope exit
/// - Select the correct cleanup strategy (reference counting, free, etc.)
/// - Create default/zero values for each type
pub trait LangType: Clone + PartialEq + std::fmt::Debug + 'static {
    /// Map to a Cranelift IR type (F64, I8, I64, etc.)
    fn ir_type(&self) -> ir::Type;

    /// Whether this type is heap-allocated (needs cleanup on scope exit).
    fn is_heap(&self) -> bool;

    /// How to clean up values of this type.
    fn cleanup(&self) -> CleanupStrategy;

    /// A sentinel "unknown/unresolved" type (typically heap pointer / I64).
    fn unknown() -> Self;

    /// The number type (F64).
    fn number() -> Self;

    /// The boolean type (I8).
    fn boolean() -> Self;
}
