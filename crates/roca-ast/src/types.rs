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
    /// Function type: fn(A, B) -> C
    Fn(Vec<TypeRef>, Box<TypeRef>),
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

    /// Convert to the shared RocaType used across all compiler stages.
    pub fn to_roca_type(&self) -> roca_types::RocaType {
        roca_types::RocaType::from(self)
    }
}

impl From<&TypeRef> for roca_types::RocaType {
    fn from(ty: &TypeRef) -> Self {
        use roca_types::RocaType;
        match ty {
            TypeRef::Number => RocaType::Number,
            TypeRef::String => RocaType::String,
            TypeRef::Bool => RocaType::Bool,
            TypeRef::Ok => RocaType::Void,
            TypeRef::Named(name) => RocaType::Struct(name.clone()),
            TypeRef::Generic(name, args) => match name.as_str() {
                "Array" => RocaType::Array(Box::new(
                    args.first().map(RocaType::from).unwrap_or(RocaType::Unknown),
                )),
                "Map" => {
                    let k = args.first().map(RocaType::from).unwrap_or(RocaType::String);
                    let v = args.get(1).map(RocaType::from).unwrap_or(RocaType::Unknown);
                    RocaType::Map(Box::new(k), Box::new(v))
                }
                "Optional" => RocaType::Optional(Box::new(
                    args.first().map(RocaType::from).unwrap_or(RocaType::Unknown),
                )),
                _ => RocaType::Struct(name.clone()),
            },
            TypeRef::Nullable(inner) => RocaType::Optional(Box::new(RocaType::from(inner.as_ref()))),
            TypeRef::Fn(params, ret) => RocaType::Fn(
                params.iter().map(RocaType::from).collect(),
                Box::new(RocaType::from(ret.as_ref())),
            ),
        }
    }
}
