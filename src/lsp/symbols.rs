use crate::ast as roca;
use tower_lsp::lsp_types::*;

pub fn document_symbols(source: &str) -> Vec<DocumentSymbol> {
    let file = match super::safe_parse(source) {
        Some(f) => f,
        None => return vec![],
    };

    let mut symbols = Vec::new();
    let mut line: u32 = 0;

    for item in &file.items {
        let (name, kind, detail) = match item {
            roca::Item::Contract(c) => {
                let methods: Vec<String> = c.functions.iter().map(|f| f.name.clone()).collect();
                (c.name.clone(), SymbolKind::INTERFACE, format!("contract ({})", methods.join(", ")))
            }
            roca::Item::Enum(e) => {
                let variants: Vec<String> = e.variants.iter().map(|v| v.name.clone()).collect();
                (e.name.clone(), SymbolKind::ENUM, format!("enum ({})", variants.join(", ")))
            }
            roca::Item::Struct(s) => {
                let fields: Vec<String> = s.fields.iter().map(|f| f.name.clone()).collect();
                (s.name.clone(), SymbolKind::CLASS, format!("struct ({})", fields.join(", ")))
            }
            roca::Item::Satisfies(sat) => {
                (format!("{} satisfies {}", sat.struct_name, sat.contract_name), SymbolKind::METHOD, "satisfies".into())
            }
            roca::Item::Function(f) => {
                let params: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
                let vis = if f.is_pub { "pub " } else { "" };
                (f.name.clone(), SymbolKind::FUNCTION, format!("{}fn({})", vis, params.join(", ")))
            }
            roca::Item::Import(imp) => {
                let source_str = match &imp.source {
                    roca::ImportSource::Path(p) => p.clone(),
                    roca::ImportSource::Std(None) => "std".into(),
                    roca::ImportSource::Std(Some(m)) => format!("std::{}", m),
                };
                (format!("import {}", imp.names.join(", ")), SymbolKind::MODULE, source_str)
            }
            roca::Item::ExternContract(c) => {
                let methods: Vec<String> = c.functions.iter().map(|f| f.name.clone()).collect();
                (c.name.clone(), SymbolKind::INTERFACE, format!("extern contract ({})", methods.join(", ")))
            }
            roca::Item::ExternFn(f) => {
                let params: Vec<String> = f.params.iter().map(|p| p.name.clone()).collect();
                (f.name.clone(), SymbolKind::FUNCTION, format!("extern fn({})", params.join(", ")))
            }
        };

        // Approximate position by searching for the name in source
        let pos = find_line_for(&name, source, &mut line);

        #[allow(deprecated)]
        symbols.push(DocumentSymbol {
            name,
            detail: Some(detail),
            kind,
            tags: None,
            deprecated: None,
            range: Range::new(Position::new(pos, 0), Position::new(pos, 80)),
            selection_range: Range::new(Position::new(pos, 0), Position::new(pos, 80)),
            children: None,
        });
    }

    symbols
}

fn find_line_for(name: &str, source: &str, last_line: &mut u32) -> u32 {
    for (i, line) in source.lines().enumerate() {
        if i as u32 >= *last_line && line.contains(name) {
            *last_line = i as u32 + 1;
            return i as u32;
        }
    }
    0
}
