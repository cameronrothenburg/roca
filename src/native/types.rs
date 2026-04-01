//! Roca type → Cranelift type mapping.

use cranelift_codegen::ir::types;
use cranelift_codegen::ir::Type;
use crate::ast::TypeRef;

/// Map a Roca type to a Cranelift IR type.
pub fn roca_to_cranelift(ty: &TypeRef) -> Type {
    match ty {
        TypeRef::Number => types::F64,
        TypeRef::Bool => types::I8,
        TypeRef::String => types::I64,
        TypeRef::Ok => types::I8,
        TypeRef::Named(_) => types::I64,
        TypeRef::Generic(_, _) => types::I64,
        TypeRef::Nullable(_) => types::I64,
        TypeRef::Fn(_, _) => types::I64, // function pointer
    }
}


