//! Search command — finds functions, structs, and contracts by name across stdlib and user source.

use std::path::Path;
use crate::ast::*;
use crate::check::walker::type_ref_to_name;

const STDLIB_SOURCE: &str = concat!(
    include_str!("../../packages/stdlib/core/traits.roca"),
    include_str!("../../packages/stdlib/core/string.roca"),
    include_str!("../../packages/stdlib/core/number.roca"),
    include_str!("../../packages/stdlib/core/bool.roca"),
    include_str!("../../packages/stdlib/core/array.roca"),
    include_str!("../../packages/stdlib/core/map.roca"),
    include_str!("../../packages/stdlib/core/optional.roca"),
    include_str!("../../packages/stdlib/core/bytes.roca"),
    include_str!("../../packages/stdlib/core/math.roca"),
    include_str!("../../packages/stdlib/core/path.roca"),
    include_str!("../../packages/stdlib/io/fs.roca"),
    include_str!("../../packages/stdlib/io/process.roca"),
);

/// A search match with display info
struct Match {
    signature: String,
    doc: Option<String>,
}

pub fn run_search(query: &str) {
    let query_lower = query.to_lowercase();
    let mut matches: Vec<Match> = Vec::new();

    // Load stdlib primitives
    let stdlib = crate::parse::parse(STDLIB_SOURCE);
    collect_matches(&stdlib, &query_lower, None, &mut matches);

    // Load stdlib modules
    for source in load_stdlib_modules() {
        if let Ok(file) = crate::parse::try_parse(&source) {
            collect_matches(&file, &query_lower, None, &mut matches);
        }
    }

    // Load user source files if in a project directory
    let src_dir = crate::cli::config::resolve_src_dir(Path::new("."));
    if src_dir.exists() {
        // Find the stdlib directory to exclude it from user file search
        let stdlib_dir = find_stdlib_dir();
        let mut roca_files = Vec::new();
        crate::cli::config::collect_roca_files(&src_dir, &mut roca_files);
        for path in &roca_files {
            // Skip files that are in the stdlib directory
            if let Some(ref stdlib) = stdlib_dir {
                if let (Ok(canon_path), Ok(canon_stdlib)) = (path.canonicalize(), stdlib.canonicalize()) {
                    if canon_path.starts_with(&canon_stdlib) {
                        continue;
                    }
                }
            }
            if let Ok(source) = std::fs::read_to_string(path) {
                if let Ok(file) = crate::parse::try_parse(&source) {
                    collect_matches(&file, &query_lower, None, &mut matches);
                }
            }
        }
    }

    if matches.is_empty() {
        println!("no matches for \"{}\"", query);
        return;
    }

    for (i, m) in matches.iter().enumerate() {
        if i > 0 {
            println!();
        }
        println!("{}", m.signature);
        if let Some(doc) = &m.doc {
            for line in doc.lines() {
                println!("  {}", line);
            }
        }
    }
}

fn collect_matches(file: &SourceFile, query: &str, _source_label: Option<&str>, matches: &mut Vec<Match>) {
    for item in &file.items {
        match item {
            Item::Contract(c) | Item::ExternContract(c) => {
                collect_contract_matches(c, query, matches);
            }
            Item::Struct(s) => {
                collect_struct_matches(s, query, matches);
            }
            Item::Function(f) => {
                if f.name.to_lowercase().contains(query) {
                    matches.push(Match {
                        signature: format_fn_sig_standalone(&f.name, f.is_pub, &f.params, &f.return_type, f.returns_err),
                        doc: f.doc.clone(),
                    });
                }
            }
            Item::ExternFn(f) => {
                if f.name.to_lowercase().contains(query) {
                    matches.push(Match {
                        signature: format_fn_sig_standalone(&f.name, false, &f.params, &f.return_type, f.returns_err),
                        doc: f.doc.clone(),
                    });
                }
            }
            Item::Enum(e) => {
                if e.name.to_lowercase().contains(query) {
                    let variants: Vec<String> = e.variants.iter().map(|v| v.name.clone()).collect();
                    matches.push(Match {
                        signature: format!("enum {} {{ {} }}", e.name, variants.join(", ")),
                        doc: e.doc.clone(),
                    });
                }
            }
            _ => {}
        }
    }
}

fn collect_contract_matches(c: &ContractDef, query: &str, matches: &mut Vec<Match>) {
    let contract_lower = c.name.to_lowercase();

    if contract_lower.contains(query) {
        matches.push(Match {
            signature: format!("contract {}", c.name),
            doc: c.doc.clone(),
        });
    }

    // Match methods as ContractName.method()
    for sig in &c.functions {
        let method_lower = sig.name.to_lowercase();
        if contract_lower.contains(query) || method_lower.contains(query) {
            matches.push(Match {
                signature: format_method_sig(&c.name, sig),
                doc: sig.doc.clone(),
            });
        }
    }

    // Match fields
    for field in &c.fields {
        if contract_lower.contains(query) || field.name.to_lowercase().contains(query) {
            matches.push(Match {
                signature: format!("{}.{}: {}", c.name, field.name, type_ref_to_name(&field.type_ref)),
                doc: None,
            });
        }
    }
}

fn collect_struct_matches(s: &StructDef, query: &str, matches: &mut Vec<Match>) {
    let struct_lower = s.name.to_lowercase();

    if struct_lower.contains(query) {
        matches.push(Match {
            signature: if s.is_pub { format!("pub struct {}", s.name) } else { format!("struct {}", s.name) },
            doc: s.doc.clone(),
        });
    }

    // Match methods
    for sig in &s.signatures {
        let method_lower = sig.name.to_lowercase();
        if struct_lower.contains(query) || method_lower.contains(query) {
            matches.push(Match {
                signature: format_method_sig(&s.name, sig),
                doc: sig.doc.clone(),
            });
        }
    }

    // Match fields
    for field in &s.fields {
        if struct_lower.contains(query) || field.name.to_lowercase().contains(query) {
            matches.push(Match {
                signature: format!("{}.{}: {}", s.name, field.name, type_ref_to_name(&field.type_ref)),
                doc: None,
            });
        }
    }
}

fn format_method_sig(owner: &str, sig: &FnSignature) -> String {
    let params = format_params(&sig.params);
    let ret = type_ref_to_name(&sig.return_type);
    if sig.returns_err {
        format!("{}.{}({}) -> {}, err", owner, sig.name, params, ret)
    } else {
        format!("{}.{}({}) -> {}", owner, sig.name, params, ret)
    }
}

fn format_fn_sig_standalone(name: &str, is_pub: bool, params: &[Param], return_type: &TypeRef, returns_err: bool) -> String {
    let p = format_params(params);
    let ret = type_ref_to_name(return_type);
    let prefix = if is_pub { "pub fn" } else { "fn" };
    if returns_err {
        format!("{} {}({}) -> {}, err", prefix, name, p, ret)
    } else {
        format!("{} {}({}) -> {}", prefix, name, p, ret)
    }
}

fn format_params(params: &[Param]) -> String {
    params.iter()
        .map(|p| format!("{}: {}", p.name, type_ref_to_name(&p.type_ref)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Find the stdlib directory (for excluding from user file search)
fn find_stdlib_dir() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;
    for base in &[
        exe_dir.join("../packages/stdlib"),
        exe_dir.join("../../packages/stdlib"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/stdlib"),
    ] {
        if base.exists() {
            return Some(base.clone());
        }
    }
    None
}

/// Load all .roca files from the stdlib directory (excluding primitives.roca which is already embedded)
fn load_stdlib_modules() -> Vec<String> {
    let mut sources = Vec::new();
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(_) => return sources,
    };
    let exe_dir = match exe.parent() {
        Some(d) => d,
        None => return sources,
    };

    let search_dirs = [
        exe_dir.join("../packages/stdlib"),
        exe_dir.join("../../packages/stdlib"),
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/stdlib"),
    ];

    for base in &search_dirs {
        if base.exists() {
            load_roca_files(base, &mut sources);
            for subdir in &["core", "io", "net", "data", "security", "time"] {
                let sub = base.join(subdir);
                if sub.exists() { load_roca_files(&sub, &mut sources); }
            }
            break;
        }
    }
    sources
}

fn load_roca_files(dir: &std::path::Path, sources: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "roca")
                && path.file_name().map_or(false, |n| n != "primitives.roca")
            {
                if let Ok(source) = std::fs::read_to_string(&path) {
                    sources.push(source);
                }
            }
        }
    }
}
