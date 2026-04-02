//! Multi-file resolution — recursively parses imports and builds a combined contract registry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use roca_ast::{SourceFile, Item, ImportSource, Param, ErrDecl, collect_returned_error_names};
use crate::registry::ContractRegistry;

/// Summary of a resolved function's signature -- used by checker rules to avoid
/// duplicating the "iterate imports, load file, find function" pattern.
#[derive(Debug, Clone)]
pub struct ResolvedFn {
    pub params: Vec<Param>,
    pub returns_err: bool,
    pub errors: Vec<ErrDecl>,
}

/// Search `file`'s imports for a function named `name`, loading .roca files
/// relative to `source_dir`. Returns the function's signature if found.
pub fn find_imported_fn(
    name: &str,
    file: &SourceFile,
    source_dir: Option<&Path>,
) -> Option<ResolvedFn> {
    for item in &file.items {
        let imp = match item {
            Item::Import(imp) => imp,
            _ => continue,
        };
        if !imp.names.iter().any(|n| n == name) {
            continue;
        }
        let path = match &imp.source {
            ImportSource::Path(p) => p,
            _ => continue,
        };
        let imported = try_load_roca_file_from(path, source_dir)?;
        for imp_item in &imported.items {
            match imp_item {
                Item::Function(f) if f.name == name => {
                    // FnDef.errors is always empty for parsed functions —
                    // error names are extracted from ReturnErr statements in the body.
                    let errors = if f.errors.is_empty() {
                        collect_returned_error_names(&f.body)
                            .into_iter()
                            .map(|name| ErrDecl { name, message: String::new() })
                            .collect()
                    } else {
                        f.errors.clone()
                    };
                    return Some(ResolvedFn {
                        params: f.params.clone(),
                        returns_err: f.returns_err,
                        errors,
                    });
                }
                Item::ExternFn(f) if f.name == name => {
                    return Some(ResolvedFn {
                        params: f.params.clone(),
                        returns_err: f.returns_err,
                        errors: f.errors.clone(),
                    });
                }
                _ => {}
            }
        }
    }
    None
}

/// Resolved project — all files parsed, combined registry built
pub struct ResolvedProject {
    pub files: HashMap<PathBuf, SourceFile>,
    pub registry: ContractRegistry,
}

/// Try to load and parse a .roca file, optionally searching from a specific directory.
pub fn try_load_roca_file_from(rel_path: &str, from_dir: Option<&Path>) -> Option<SourceFile> {
    let roca_path = Path::new(rel_path);
    let mut bases: Vec<PathBuf> = vec![PathBuf::from("."), PathBuf::from("src")];
    if let Some(dir) = from_dir {
        bases.insert(0, dir.to_path_buf());
        bases.insert(1, dir.join("src"));
    }
    for base in &bases {
        let full_path = base.join(roca_path);
        if let Ok(source) = std::fs::read_to_string(&full_path) {
            if let Ok(file) = roca_parse::try_parse(&source) {
                return Some(file);
            }
        }
    }
    None
}

/// Resolve a file and all its imports recursively.
/// Returns a combined registry that includes stdlib + all imported contracts/structs.
pub fn resolve_file(path: &Path) -> ResolvedProject {
    let mut files: HashMap<PathBuf, SourceFile> = HashMap::new();
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    // Recursively parse the file and its imports
    collect_file(&canonical, &mut files);

    // Build a combined registry from all files
    let registry = build_combined_registry(&files);

    ResolvedProject { files, registry }
}

/// Resolve all files in a directory
pub fn resolve_directory(dir: &Path) -> ResolvedProject {
    let mut files: HashMap<PathBuf, SourceFile> = HashMap::new();

    // Find all .roca files
    let mut roca_files = Vec::new();
    collect_roca_paths(dir, &mut roca_files);

    // Parse each and follow imports
    for path in &roca_files {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        collect_file(&canonical, &mut files);
    }

    let registry = build_combined_registry(&files);

    ResolvedProject { files, registry }
}

fn collect_file(path: &Path, files: &mut HashMap<PathBuf, SourceFile>) {
    if files.contains_key(path) {
        return; // Already parsed — avoid cycles
    }

    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error reading {}: {}", path.display(), e);
            return;
        }
    };

    let file = match roca_parse::try_parse(&source) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {}", path.display(), e);
            return;
        }
    };

    // Follow imports
    for item in &file.items {
        if let roca_ast::Item::Import(imp) = item {
            if let ImportSource::Path(ref import_path) = imp.source {
                let resolved = resolve_import_path(path, import_path);
                collect_file(&resolved, files);
            }
            // std:: imports are handled by the registry automatically
        }
    }

    files.insert(path.to_path_buf(), file);
}

fn resolve_import_path(from_file: &Path, import_path: &str) -> PathBuf {
    let dir = from_file.parent().unwrap_or(Path::new("."));
    let mut resolved = dir.join(import_path);

    // If the path ends with .js, try .roca instead
    if resolved.extension().map_or(false, |e| e == "js") {
        resolved.set_extension("roca");
    }

    resolved.canonicalize().unwrap_or(resolved)
}

fn build_combined_registry(files: &HashMap<PathBuf, SourceFile>) -> ContractRegistry {
    // Build from the first file (loads stdlib automatically), then add the rest
    let mut iter = files.values();
    let mut registry = match iter.next() {
        Some(first) => ContractRegistry::build(first),
        None => ContractRegistry::build(&SourceFile { items: Vec::new() }),
    };
    for file in iter {
        registry.load_file(file);
    }

    registry
}

fn collect_roca_paths(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().map_or(false, |n| n == "out") {
                    continue;
                }
                collect_roca_paths(&path, files);
            } else if path.extension().map_or(false, |e| e == "roca") {
                files.push(path);
            }
        }
    }
}
