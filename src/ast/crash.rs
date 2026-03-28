use super::expr::Expr;

/// Crash block inside a function — one handler per call site
/// ```roca
/// crash {
///     http.get {
///         err.timeout -> retry(3, 1000)
///         err.not_found -> fallback(empty)
///         default -> halt
///     }
///     db.save -> retry(1, 500)
/// }
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct CrashBlock {
    pub handlers: Vec<CrashHandler>,
}

/// Handler for a single call site
#[derive(Debug, Clone, PartialEq)]
pub struct CrashHandler {
    /// The call being handled, e.g. "http.get" or "Email.validate"
    pub call: String,
    /// Strategy or per-error strategies
    pub strategy: CrashHandlerKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CrashHandlerKind {
    /// Single strategy for all errors from this call
    Simple(CrashStrategy),
    /// Per-error strategies with optional default
    Detailed {
        arms: Vec<CrashArm>,
        default: Option<CrashStrategy>,
    },
}

/// A specific error -> strategy mapping
#[derive(Debug, Clone, PartialEq)]
pub struct CrashArm {
    /// err.name reference
    pub err_name: String,
    pub strategy: CrashStrategy,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CrashStrategy {
    /// retry(attempts, delay_ms)
    Retry { attempts: u32, delay_ms: u32 },
    /// skip — ignore failure, move on
    Skip,
    /// halt — propagate error to caller
    Halt,
    /// fallback(value) — use a default
    Fallback(Expr),
}
