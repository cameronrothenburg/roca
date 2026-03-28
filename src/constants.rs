/// All Roca keywords — single source of truth.
/// Used by tokenizer, LSP completion, syntax highlighting.
pub const KEYWORDS: &[&str] = &[
    "contract", "struct", "satisfies",
    "fn", "pub", "const", "let", "return",
    "if", "else", "for", "in", "match",
    "crash", "test", "mock",
    "err", "Ok",
    "retry", "skip", "halt", "fallback", "default",
    "import", "from", "std",
    "self", "is",
    "true", "false",
    "log", "error", "warn",
    "wait", "waitAll", "waitFirst",
];

/// Built-in type names.
pub const BUILTIN_TYPES: &[&str] = &[
    "String", "Number", "Bool", "Array", "Map", "Bytes", "Loggable",
];

/// Crash strategy keywords.
pub const CRASH_STRATEGIES: &[&str] = &[
    "retry", "skip", "halt", "fallback",
];

/// Console builtins that require Loggable.
pub const CONSOLE_BUILTINS: &[&str] = &["log", "error", "warn"];
