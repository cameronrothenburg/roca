//! Language constants — keywords, built-in types, reserved names, and console builtins.
//! Single source of truth for the tokenizer, LSP, and syntax highlighting.

/// All Roca keywords (39) — matches spec §1.4.
/// Used by tokenizer, LSP completion, syntax highlighting.
pub const KEYWORDS: &[&str] = &[
    // Declarations
    "contract", "struct", "enum", "extern", "satisfies", "fn", "pub",
    // Bindings
    "const", "let",
    // Control flow
    "return", "if", "else", "for", "in", "match", "while", "break", "continue",
    // Blocks
    "crash", "test",
    // Error handling
    "err", "Ok", "null",
    // Crash strategies
    "retry", "skip", "halt", "fallback", "panic", "default",
    // Async
    "wait", "waitAll", "waitFirst",
    // Modules
    "import", "from", "std",
    // Identity
    "self", "is",
    // Literals
    "true", "false",
];

/// Built-in type names — primitives recognized by the type system.
pub const BUILTIN_TYPES: &[&str] = &[
    "String", "Number", "Bool", "Ok",
];

/// Reserved stdlib contract names — user code MUST NOT define types with these names.
/// Matches spec §4.2.2.
pub const RESERVED_NAMES: &[&str] = &[
    "String", "Number", "Bool", "Array", "Map", "Optional", "Bytes", "Buffer",
    "Math", "JSON", "Fs", "Http", "Url", "Crypto", "Encoding", "Time",
    "Path", "Char", "NumberParse", "Process",
    "Loggable", "Serializable", "Deserializable",
];

/// Console builtins that require Loggable.
pub const CONSOLE_BUILTINS: &[&str] = &["log", "error", "warn"];
