//! Error types and diagnostic codes for the Roca compiler.
//! Defines all rule error codes and the `RuleError`/`ParseError` structs.

// Rule error codes — single source of truth for all checker diagnostics.
// Crash rules
pub const MISSING_CRASH: &str = "missing-crash";
pub const UNHANDLED_CALL: &str = "unhandled-call";
pub const CRASH_ON_SAFE: &str = "crash-on-safe";
pub const PANIC_WARNING: &str = "panic-warning";
// Contract rules
pub const DUPLICATE_ERR: &str = "duplicate-err";
pub const ERR_NO_ERRORS: &str = "err-no-errors";
// Struct rules
pub const EMPTY_STRUCT: &str = "empty-struct";
pub const MISSING_IMPL: &str = "missing-impl";
pub const SIG_MISMATCH: &str = "sig-mismatch";
pub const UNDECLARED_METHOD: &str = "undeclared-method";
// Satisfies rules
pub const UNKNOWN_CONTRACT: &str = "unknown-contract";
pub const MISSING_SATISFIES: &str = "missing-satisfies";
pub const SATISFIES_MISMATCH: &str = "satisfies-mismatch";
// Test rules
pub const MISSING_TEST: &str = "missing-test";
pub const UNTESTED_ERROR: &str = "untested-error";
pub const NO_SUCCESS_TEST: &str = "no-success-test";
pub const TEST_SHAPE_MISMATCH: &str = "test-shape-mismatch";
// Variable rules
pub const CONST_REASSIGN: &str = "const-reassign";
// Type rules
pub const NULLABLE_TYPE: &str = "nullable-type";
pub const NULLABLE_RETURN: &str = "nullable-return";
pub const RETURN_TYPE_MISMATCH: &str = "return-type-mismatch";
pub const RETURN_NULL: &str = "return-null";
pub const RETURN_ERR_NOT_DECLARED: &str = "return-err-not-declared";
pub const TYPE_ANNOTATION_MISMATCH: &str = "type-annotation-mismatch";
pub const FIELD_TYPE_MISMATCH: &str = "field-type-mismatch";
pub const UNKNOWN_FIELD: &str = "unknown-field";
pub const ARG_TYPE_MISMATCH: &str = "arg-type-mismatch";
// Method rules
pub const NULLABLE_ACCESS: &str = "nullable-access";
pub const UNKNOWN_METHOD: &str = "unknown-method";
pub const PRIVATE_METHOD: &str = "private-method";
pub const GENERIC_MISMATCH: &str = "generic-mismatch";
pub const CONSTRAINT_VIOLATION: &str = "constraint-violation";
pub const TYPE_MISMATCH: &str = "type-mismatch";
pub const STRUCT_COMPARISON: &str = "struct-comparison";
pub const INVALID_ORDERING: &str = "invalid-ordering";
pub const NOT_LOGGABLE: &str = "not-loggable";
// Unhandled error rules
pub const UNHANDLED_ERROR: &str = "unhandled-error";
// Constraint rules
pub const INVALID_CONSTRAINT: &str = "invalid-constraint";
pub const MISSING_DEFAULT: &str = "missing-default";
// Manual error rules
pub const ERR_IN_BODY: &str = "err-in-body";
pub const MANUAL_ERR_CHECK: &str = "manual-err-check";
// Doc rules
pub const MISSING_DOC: &str = "missing-doc";
// Reserved name rules
pub const RESERVED_NAME: &str = "reserved-name";
// Test rules
pub const OK_ON_INFALLIBLE: &str = "ok-on-infallible";
pub const SELF_REFERENTIAL_TEST: &str = "self-referential-test";
// Crash rules (chain validation)
pub const NONTERMINAL_CHAIN: &str = "nonterminal-chain";
// Ownership rules
pub const USE_AFTER_MOVE: &str = "use-after-move";
pub const MOVE_IN_LOOP: &str = "move-in-loop";
pub const MUST_BE_CONST: &str = "must-be-const";
pub const RECURSIVE_CYCLE: &str = "recursive-cycle";

#[derive(Debug, Clone)]
pub struct RuleError {
    pub code: String,
    pub message: String,
    pub context: Option<String>,
}

impl RuleError {
    pub fn new(code: &'static str, message: impl Into<String>, context: Option<String>) -> Self {
        Self { code: code.into(), message: message.into(), context }
    }
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "error[{}]: {}", self.code, self.message)?;
        if let Some(ctx) = &self.context {
            write!(f, "\n  → {}", ctx)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub pos: usize,
}

impl ParseError {
    pub fn new(message: impl Into<String>, pos: usize) -> Self {
        Self { message: message.into(), pos }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error at token {}: {}", self.pos, self.message)
    }
}

impl std::error::Error for ParseError {}
