//! Shared type system for the Roca compiler.
//! Used by all stages: parser, checker, JS emitter, and native backend.
//! Each stage adds its own extension traits for stage-specific behavior.
//!
//! The type system is open — extern contracts and user-defined types
//! are first-class. There are no hardcoded runtime types; stdlib types
//! like Json or Url are just `Struct("Json")` with cleanup behavior
//! registered at the backend level.

/// The Roca type system — single source of truth across all compiler stages.
///
/// Open by design: `Struct(name)` handles both built-in types (Json, Url, etc.)
/// and user-defined extern contracts (Redis, Stripe, etc.) uniformly.
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

    /// Named type — structs, extern contracts, and runtime types (Json, Url, etc.).
    /// Cleanup behavior is determined by the backend via registered strategies,
    /// not by the type system. This keeps the enum open to user-defined types.
    Struct(std::string::String),

    /// Algebraic enum — tagged union with variant names.
    Enum(std::string::String),

    // Composite
    Optional(Box<RocaType>),
    Fn(Vec<RocaType>, Box<RocaType>),

    // Escape hatch for unresolvable types
    Unknown,
}

/// Cleanup strategy for heap-managed types.
/// Registered per-type at the backend level, not hardcoded in the enum.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CleanupStrategy {
    /// RC-managed string — decrement refcount, free at zero.
    RcRelease,
    /// Array — free the Vec<i64>.
    FreeArray,
    /// Struct — release heap fields then free the Vec<i64>.
    FreeStruct { heap_fields: u32 },
    /// Enum variant — tagged struct with 1 heap field (the tag string).
    FreeEnum,
    /// Box-allocated opaque type — call drop trampoline + dealloc.
    BoxFree,
    /// No cleanup needed (stack value, or managed elsewhere).
    None,
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

    /// Get the element type for containers (Array<T> → T).
    pub fn element_type(&self) -> Option<&RocaType> {
        match self {
            RocaType::Array(inner) | RocaType::Optional(inner) => Some(inner),
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
            RocaType::Struct(name) | RocaType::Enum(name) => name,
            RocaType::Optional(_) => "Optional",
            RocaType::Fn(_, _) => "Fn",
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

    /// Default cleanup strategy for this type.
    /// Backends can override this per struct name for special types (Json, Url, etc.).
    pub fn default_cleanup(&self) -> CleanupStrategy {
        match self {
            RocaType::String => CleanupStrategy::RcRelease,
            RocaType::Array(_) => CleanupStrategy::FreeArray,
            RocaType::Map(_, _) | RocaType::Struct(_) => CleanupStrategy::FreeStruct { heap_fields: 0 },
            RocaType::Enum(_) => CleanupStrategy::FreeEnum,
            _ => CleanupStrategy::None,
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
    }

    #[test]
    fn extern_types_are_heap() {
        // User-defined extern contracts are heap-managed
        assert!(RocaType::Struct("Redis".into()).is_heap());
        assert!(RocaType::Struct("Stripe".into()).is_heap());
    }

    #[test]
    fn runtime_types_are_just_structs() {
        // Json, Url, HttpResponse are just Struct("Json"), etc.
        let json = RocaType::Struct("Json".into());
        let url = RocaType::Struct("Url".into());
        assert!(json.is_heap());
        assert!(url.is_heap());
        assert_eq!(json.base_name(), "Json");
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
    fn default_cleanup_strategies() {
        assert_eq!(RocaType::String.default_cleanup(), CleanupStrategy::RcRelease);
        assert_eq!(RocaType::Array(Box::new(RocaType::Number)).default_cleanup(), CleanupStrategy::FreeArray);
        assert_eq!(RocaType::Struct("Email".into()).default_cleanup(), CleanupStrategy::FreeStruct { heap_fields: 0 });
        assert_eq!(RocaType::Enum("Token".into()).default_cleanup(), CleanupStrategy::FreeEnum);
        assert_eq!(RocaType::Number.default_cleanup(), CleanupStrategy::None);
    }

    #[test]
    fn base_names() {
        assert_eq!(RocaType::Number.base_name(), "Number");
        assert_eq!(RocaType::Struct("Email".into()).base_name(), "Email");
        assert_eq!(RocaType::Array(Box::new(RocaType::Number)).base_name(), "Array");
    }
}
