//! Crash block AST nodes — error recovery strategies (retry, skip, halt, fallback).

use super::expr::Expr;

/// Crash block inside a function — one handler per call site
#[derive(Debug, Clone, PartialEq)]
pub struct CrashBlock {
    pub handlers: Vec<CrashHandler>,
}

/// Handler for a single call site
#[derive(Debug, Clone, PartialEq)]
pub struct CrashHandler {
    pub call: String,
    pub strategy: CrashHandlerKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CrashHandlerKind {
    /// Single chain for all errors: log |> retry(3, 1000) |> halt
    Simple(CrashChain),
    /// Per-error chains with optional default
    Detailed {
        arms: Vec<CrashArm>,
        default: Option<CrashChain>,
    },
}

/// A specific error -> chain mapping
#[derive(Debug, Clone, PartialEq)]
pub struct CrashArm {
    pub err_name: String,
    pub chain: CrashChain,
}

/// A chain of crash steps: log |> retry(3, 1000) |> halt
pub type CrashChain = Vec<CrashStep>;

#[derive(Debug, Clone, PartialEq)]
pub enum CrashStep {
    /// log — log the error, continue chain
    Log,
    /// panic — crash the process
    Panic,
    /// halt — propagate error to caller
    Halt,
    /// skip — swallow error, continue execution
    Skip,
    /// retry(attempts, delay_ms)
    Retry { attempts: u32, delay_ms: u32 },
    /// fallback(value) — use a default
    Fallback(Expr),
}
