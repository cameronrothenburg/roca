/// All Roca keywords — single source of truth.
/// Used by tokenizer, LSP completion, syntax highlighting.
pub const KEYWORDS: &[&str] = &[
    "contract", "struct", "enum", "satisfies",
    "extern", "panic",
    "fn", "pub", "const", "let", "return",
    "if", "else", "for", "in", "match", "while", "break", "continue",
    "crash", "test", "mock",
    "err", "Ok",
    "retry", "skip", "halt", "fallback", "default",
    "import", "from", "std",
    "self", "is",
    "null", "true", "false",
    "log", "error", "warn",
    "wait", "waitAll", "waitFirst",
];

/// Built-in type names.
pub const BUILTIN_TYPES: &[&str] = &[
    "String", "Number", "Bool", "Array", "Map", "Bytes", "Loggable", "Optional",
];

/// Console builtins that require Loggable.
pub const CONSOLE_BUILTINS: &[&str] = &["log", "error", "warn"];
