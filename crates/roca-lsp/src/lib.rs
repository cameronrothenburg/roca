//! Language Server Protocol implementation for Roca, built on `tower-lsp`.
//!
//! Depends on [`roca_ast`], [`roca_parse`], [`roca_check`], [`roca_errors`],
//! and [`roca_resolve`]. Provides real-time diagnostics, completions, and
//! document symbols to editors that speak LSP.
//!
//! # Key exports
//!
//! - [`run()`] — start the LSP server on stdin/stdout (called by `roca lsp`).
//!
//! Internally, the server re-parses on every `textDocument/didChange` via
//! [`safe_parse()`] (which swallows syntax errors to keep the server alive),
//! then runs `roca_check` to produce diagnostics.

mod backend;
mod completion;
mod diagnostics;
mod symbols;

use roca_ast::SourceFile;

/// Parse source safely — returns None on error instead of crashing the LSP.
pub(crate) fn safe_parse(source: &str) -> Option<SourceFile> {
    roca_parse::try_parse(source).ok()
}

use tower_lsp::{LspService, Server};

pub async fn run() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(backend::Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
