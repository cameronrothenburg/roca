//! Autocompletion provider — suggests keywords, types, and contract methods at the cursor.

use crate::check::registry::ContractRegistry;
use crate::constants::{KEYWORDS, BUILTIN_TYPES};
use tower_lsp::lsp_types::*;

const STDLIB_MODULES: &[(&str, &str)] = &[
    ("json", "JSON parsing and serialization"),
    ("http", "HTTP requests (get, post, put, patch, delete)"),
    ("url", "URL parsing and query parameters"),
    ("crypto", "Cryptographic operations (UUID, SHA)"),
    ("encoding", "Text encoding/decoding and base64"),
    ("time", "Timestamps and date parsing"),
];

pub fn completions(source: &str, position: Position) -> Vec<CompletionItem> {
    let line_idx = position.line as usize;
    let col = position.character as usize;

    let line = source.lines().nth(line_idx).unwrap_or("");
    let before_cursor = if col <= line.len() { &line[..col] } else { line };

    // After a dot — suggest methods for the type
    if let Some(dot_pos) = before_cursor.rfind('.') {
        let before_dot = before_cursor[..dot_pos].trim();
        let ident = before_dot.rsplit(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or("");

        if !ident.is_empty() {
            let registry = build_registry(source);
            let type_name = infer_type_from_source(ident, source, &registry);

            if let Some(type_name) = type_name {
                let methods = registry.available_methods(&type_name);
                return methods.iter().map(|m| CompletionItem {
                    label: m.clone(),
                    kind: Some(CompletionItemKind::METHOD),
                    detail: Some(format!("{}.{}", type_name, m)),
                    ..Default::default()
                }).collect();
            }
        }

        // After "err." — suggest error names
        if before_dot.ends_with("err") || before_dot.trim_end().ends_with("err") {
            return suggest_error_names(source);
        }
    }

    // After "from std::" — suggest stdlib modules
    if before_cursor.contains("from std::") {
        return STDLIB_MODULES.iter().map(|(name, desc)| CompletionItem {
            label: name.to_string(),
            kind: Some(CompletionItemKind::MODULE),
            detail: Some(desc.to_string()),
            ..Default::default()
        }).collect();
    }

    // After "from " — suggest std
    if before_cursor.trim_end().ends_with("from") {
        return vec![
            CompletionItem {
                label: "std".into(),
                kind: Some(CompletionItemKind::MODULE),
                detail: Some("Standard library".into()),
                ..Default::default()
            },
        ];
    }

    // After "-> " — suggest types
    if before_cursor.contains("->") {
        return BUILTIN_TYPES.iter().map(|t| CompletionItem {
            label: t.to_string(),
            kind: Some(CompletionItemKind::TYPE_PARAMETER),
            ..Default::default()
        }).collect();
    }

    // Default: keywords + types
    let mut items: Vec<CompletionItem> = KEYWORDS.iter().map(|k| CompletionItem {
        label: k.to_string(),
        kind: Some(CompletionItemKind::KEYWORD),
        ..Default::default()
    }).collect();

    items.extend(BUILTIN_TYPES.iter().map(|t| CompletionItem {
        label: t.to_string(),
        kind: Some(CompletionItemKind::TYPE_PARAMETER),
        ..Default::default()
    }));

    items
}

fn build_registry(source: &str) -> ContractRegistry {
    match super::safe_parse(source) {
        Some(file) => ContractRegistry::build(&file),
        None => ContractRegistry::build(&crate::parse::parse("")),
    }
}

fn infer_type_from_source(ident: &str, source: &str, registry: &ContractRegistry) -> Option<String> {
    // Check if ident is a known type name
    if registry.get(ident).is_some() {
        return Some(ident.to_string());
    }

    // Search source for `let/const ident = TypeName(...)` or `: TypeName`
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.contains(ident) {
            // Pattern: ident: TypeName
            if let Some(colon_pos) = trimmed.find(&format!("{}: ", ident)) {
                let after = &trimmed[colon_pos + ident.len() + 2..];
                let type_name: String = after.chars().take_while(|c| c.is_alphanumeric()).collect();
                if !type_name.is_empty() {
                    return Some(type_name);
                }
            }
        }
    }

    None
}

fn suggest_error_names(source: &str) -> Vec<CompletionItem> {
    let mut names = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("err ") && trimmed.contains('=') {
            let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
            if parts.len() >= 2 {
                let name = parts[1].trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_');
                names.push(CompletionItem {
                    label: name.to_string(),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    detail: Some("error name".into()),
                    ..Default::default()
                });
            }
        }
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, col: u32) -> Position {
        Position::new(line, col)
    }

    fn labels(items: &[CompletionItem]) -> Vec<String> {
        items.iter().map(|i| i.label.clone()).collect()
    }

    #[test]
    fn after_from_suggests_std() {
        let items = completions("import { JSON } from ", pos(0, 21));
        assert!(labels(&items).contains(&"std".to_string()));
    }

    #[test]
    fn after_from_std_suggests_modules() {
        let items = completions("import { Http } from std::", pos(0, 26));
        let names = labels(&items);
        assert!(names.contains(&"json".to_string()));
        assert!(names.contains(&"http".to_string()));
        assert!(names.contains(&"url".to_string()));
        assert!(names.contains(&"crypto".to_string()));
        assert!(names.contains(&"encoding".to_string()));
        assert!(names.contains(&"time".to_string()));
    }

    #[test]
    fn after_arrow_suggests_types() {
        let items = completions("pub fn test() -> ", pos(0, 17));
        let names = labels(&items);
        assert!(names.contains(&"String".to_string()));
        assert!(names.contains(&"Number".to_string()));
        assert!(names.contains(&"Bool".to_string()));
    }

    #[test]
    fn after_err_dot_suggests_error_names() {
        let src = "pub fn test() -> String, err {\n    err missing = \"required\"\n    err invalid = \"bad\"\n    return err.";
        let items = completions(src, pos(3, 15));
        let names = labels(&items);
        assert!(names.contains(&"missing".to_string()));
        assert!(names.contains(&"invalid".to_string()));
    }

    #[test]
    fn default_suggests_keywords() {
        let items = completions("", pos(0, 0));
        let names = labels(&items);
        assert!(names.contains(&"pub".to_string()));
        assert!(names.contains(&"fn".to_string()));
        assert!(names.contains(&"struct".to_string()));
        assert!(names.contains(&"contract".to_string()));
    }
}
