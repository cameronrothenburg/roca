//! Roca type → Cranelift type mapping.

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::Type;
use crate::ast::TypeRef;

/// Map a Roca type to a Cranelift IR type.
/// Strings and complex types are pointers (I64 on 64-bit).
pub fn roca_to_cranelift(ty: &TypeRef) -> Type {
    match ty {
        TypeRef::Number => types::F64,
        TypeRef::Bool => types::I8,
        TypeRef::String => types::I64, // pointer to heap string
        TypeRef::Ok => types::I8,      // 0 = success
        TypeRef::Named(_) => types::I64, // pointer to heap object
        TypeRef::Generic(_, _) => types::I64, // pointer
        TypeRef::Nullable(_) => types::I64, // pointer (null = 0)
    }
}

/// The pointer type for the target platform.
pub fn ptr_type() -> Type {
    types::I64
}

/// The type used for error tags in {value, err} returns.
/// 0 = no error, >0 = error code.
pub fn err_tag_type() -> Type {
    types::I8
}
