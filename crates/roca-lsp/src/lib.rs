//! Language Server Protocol implementation for Roca.
//! Provides diagnostics, completions, and document symbols via `tower-lsp`.

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
