use roca::errors::RuleError;
use tower_lsp::lsp_types::*;

/// Parse and check a Roca source file, returning LSP diagnostics.
pub fn check_source(source: &str) -> Vec<Diagnostic> {
    let errors = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let file = roca::parse::parse(source);
        roca::check::check(&file)
    })) {
        Ok(errors) => errors,
        Err(_) => {
            return vec![Diagnostic {
                range: Range::new(Position::new(0, 0), Position::new(0, 0)),
                severity: Some(DiagnosticSeverity::ERROR),
                message: "Parse error — syntax may be incomplete or invalid".into(),
                ..Default::default()
            }];
        }
    };

    errors.iter().map(|e| rule_error_to_diagnostic(e, source)).collect()
}

fn rule_error_to_diagnostic(err: &RuleError, source: &str) -> Diagnostic {
    // Try to find approximate position by searching for context in source
    let range = find_error_range(err, source);

    let mut message = err.message.clone();
    if let Some(ctx) = &err.context {
        message = format!("{}\n  → {}", message, ctx);
    }

    Diagnostic {
        range,
        severity: Some(DiagnosticSeverity::ERROR),
        code: Some(NumberOrString::String(err.code.clone())),
        source: Some("roca".into()),
        message,
        ..Default::default()
    }
}

/// Try to find where the error is in the source by searching for the relevant identifier.
fn find_error_range(err: &RuleError, source: &str) -> Range {
    // Extract a searchable term from the error
    let search_term = extract_search_term(err);

    if let Some(term) = search_term {
        for (line_num, line) in source.lines().enumerate() {
            if let Some(col) = line.find(&term) {
                return Range::new(
                    Position::new(line_num as u32, col as u32),
                    Position::new(line_num as u32, (col + term.len()) as u32),
                );
            }
        }
    }

    // Fallback: first line
    Range::new(Position::new(0, 0), Position::new(0, 0))
}

fn extract_search_term(err: &RuleError) -> Option<String> {
    match err.code.as_str() {
        "missing-test" | "missing-crash" => {
            // "function 'greet' has no test block" → search for "fn greet"
            if let Some(name) = err.message.split('\'').nth(1) {
                return Some(format!("fn {}", name));
            }
        }
        "missing-impl" | "undeclared-method" => {
            // Context has "StructName.method"
            if let Some(ctx) = &err.context {
                if let Some(name) = ctx.split('.').last() {
                    return Some(name.to_string());
                }
            }
        }
        "unknown-contract" | "missing-satisfies" | "satisfies-mismatch" => {
            if let Some(name) = err.message.split('\'').nth(1) {
                return Some(format!("satisfies {}", name));
            }
        }
        "const-reassign" => {
            if let Some(name) = err.message.split('\'').nth(1) {
                return Some(name.to_string());
            }
        }
        "unknown-method" | "not-loggable" => {
            if let Some(name) = err.message.split('\'').nth(1) {
                return Some(name.to_string());
            }
        }
        "duplicate-err" => {
            if let Some(name) = err.message.split('\'').nth(1) {
                return Some(format!("err {}", name));
            }
        }
        _ => {}
    }
    None
}
