use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::ast::{SourceFile, ImportSource};
use crate::check::registry::ContractRegistry;

/// Resolved project — all files parsed, combined registry built
pub struct ResolvedProject {
    pub files: HashMap<PathBuf, SourceFile>,
    pub registry: ContractRegistry,
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

    let file = match crate::parse::try_parse(&source) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{}: {}", path.display(), e);
            return;
        }
    };

    // Follow imports
    for item in &file.items {
        if let crate::ast::Item::Import(imp) = item {
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
