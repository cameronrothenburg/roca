//! Type reference AST nodes — String, Number, Bool, Named, Generic, Nullable, Ok.

/// Type references in Roca
#[derive(Debug, Clone, PartialEq)]
pub enum TypeRef {
    String,
    Number,
    Bool,
    Named(String),
    /// Generic type: Array<Email>, Map<String, Number>
    Generic(String, Vec<TypeRef>),
    /// Type | null — nullable field
    Nullable(Box<TypeRef>),
    Ok,
}

impl TypeRef {
    pub fn from_str(s: &str) -> Self {
        match s {
            "String" => TypeRef::String,
            "Number" => TypeRef::Number,
            "Bool" => TypeRef::Bool,
            "Ok" => TypeRef::Ok,
            other => TypeRef::Named(other.to_string()),
        }
    }
}
