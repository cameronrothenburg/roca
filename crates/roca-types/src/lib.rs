//! Shared type system for the Roca compiler.
//! Used by all stages: parser, checker, JS emitter, and native backend.
//! Each stage adds its own extension traits for stage-specific behavior.

/// The Roca type system — single source of truth across all compiler stages.
#[derive(Debug, Clone, PartialEq)]
pub enum RocaType {
    // Primitives (stack-allocated, no cleanup)
    Number,
    Bool,
    Void,

    // Heap-managed (need cleanup at scope exit)
    String,
    Array(Box<RocaType>),
    Map(Box<RocaType>, Box<RocaType>),
    Struct(std::string::String),
    Enum(std::string::String),

    // Composite
    Optional(Box<RocaType>),
    Fn(Vec<RocaType>, Box<RocaType>),

    // Runtime-inferred (not in source — produced by stdlib calls)
    Json,
    Url,
    HttpResponse,
    JsonArray,

    // Escape hatch for unresolvable types
    Unknown,
}

impl RocaType {
    /// Does this type live on the heap and need cleanup at scope exit?
    pub fn is_heap(&self) -> bool {
        matches!(
            self,
            RocaType::String
                | RocaType::Array(_)
                | RocaType::Map(_, _)
                | RocaType::Struct(_)
                | RocaType::Enum(_)
                | RocaType::Json
                | RocaType::Url
                | RocaType::HttpResponse
                | RocaType::JsonArray
        )
    }

    /// Is this a simple stack-allocated primitive?
    pub fn is_primitive(&self) -> bool {
        matches!(self, RocaType::Number | RocaType::Bool | RocaType::Void)
    }

    /// Is this an optional/nullable type?
    pub fn is_nullable(&self) -> bool {
        matches!(self, RocaType::Optional(_))
    }

    /// Is this a boxed opaque type (Json, Url, HttpResponse)?
    pub fn is_boxed(&self) -> bool {
        matches!(
            self,
            RocaType::Json | RocaType::Url | RocaType::HttpResponse
        )
    }

    /// Get the element type for containers (Array<T> → T).
    pub fn element_type(&self) -> Option<&RocaType> {
        match self {
            RocaType::Array(inner) => Some(inner),
            RocaType::Optional(inner) => Some(inner),
            _ => None,
        }
    }

    /// Short name for display and diagnostics.
    pub fn base_name(&self) -> &str {
        match self {
            RocaType::Number => "Number",
            RocaType::Bool => "Bool",
            RocaType::Void => "Void",
            RocaType::String => "String",
            RocaType::Array(_) => "Array",
            RocaType::Map(_, _) => "Map",
            RocaType::Struct(name) => name,
            RocaType::Enum(name) => name,
            RocaType::Optional(_) => "Optional",
            RocaType::Fn(_, _) => "Fn",
            RocaType::Json => "Json",
            RocaType::Url => "Url",
            RocaType::HttpResponse => "HttpResponse",
            RocaType::JsonArray => "JsonArray",
            RocaType::Unknown => "Unknown",
        }
    }

    /// Unwrap Optional to the inner type, or return self.
    pub fn unwrap_optional(&self) -> &RocaType {
        match self {
            RocaType::Optional(inner) => inner,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primitives_are_not_heap() {
        assert!(!RocaType::Number.is_heap());
        assert!(!RocaType::Bool.is_heap());
        assert!(!RocaType::Void.is_heap());
    }

    #[test]
    fn heap_types_are_heap() {
        assert!(RocaType::String.is_heap());
        assert!(RocaType::Array(Box::new(RocaType::Number)).is_heap());
        assert!(RocaType::Struct("Email".into()).is_heap());
        assert!(RocaType::Json.is_heap());
        assert!(RocaType::HttpResponse.is_heap());
    }

    #[test]
    fn element_type_extracts_inner() {
        let arr = RocaType::Array(Box::new(RocaType::String));
        assert_eq!(arr.element_type(), Some(&RocaType::String));
        assert_eq!(RocaType::Number.element_type(), None);
    }

    #[test]
    fn unknown_is_not_heap() {
        assert!(!RocaType::Unknown.is_heap());
    }

    #[test]
    fn base_names() {
        assert_eq!(RocaType::Number.base_name(), "Number");
        assert_eq!(RocaType::Struct("Email".into()).base_name(), "Email");
        assert_eq!(RocaType::Array(Box::new(RocaType::Number)).base_name(), "Array");
    }
}
