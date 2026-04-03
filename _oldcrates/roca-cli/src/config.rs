//! Project configuration — resolves src/out directories from `roca.toml` and builds source files.

use std::fs;
use std::path::{Path, PathBuf};

use roca_ast as ast;
use roca_parse as parse;
use roca_check as check;
use roca_resolve as resolve;

/// Resolve source directory from roca.toml or default to current dir
pub fn resolve_src_dir(project_dir: &Path) -> PathBuf {
    let config_path = project_dir.join("roca.toml");
    if config_path.exists() {
        if let Ok(content) = fs::read_to_string(&config_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("src") && trimmed.contains('=') {
                    if let Some(val) = trimmed.split('=').nth(1) {
                        let dir = val.trim().trim_matches('"').trim_end_matches('/');
                        let src = project_dir.join(dir);
                        if src.exists() {
                            return src;
                        }
                    }
                }
            }
        }
    }
    project_dir.to_path_buf()
}

pub fn resolve_out_dir(path: &Path) -> PathBuf {
    let start_dir = if path.is_dir() { path } else { path.parent().unwrap_or(Path::new(".")) };

    let mut search_dir = start_dir;
    loop {
        let config_path = search_dir.join("roca.toml");
        if config_path.exists() {
            if let Ok(content) = fs::read_to_string(&config_path) {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("out") && trimmed.contains('=') {
                        if let Some(val) = trimmed.split('=').nth(1) {
                            let dir = val.trim().trim_matches('"').trim_end_matches('/');
                            return search_dir.join(dir);
                        }
                    }
                }
            }
            break;
        }
        match search_dir.parent() {
            Some(parent) if parent != search_dir => search_dir = parent,
            _ => break,
        }
    }

    start_dir.join("out")
}

pub fn output_path_for(source_path: &Path, src_dir: &Path, out_dir: &Path) -> PathBuf {
    let relative = source_path.strip_prefix(src_dir).unwrap_or(source_path);
    out_dir.join(relative).with_extension("js")
}

/// Resolve a single file from a project — returns parsed+checked SourceFile or error string
pub fn resolve_file_from_project(path: &Path, project: &resolve::ResolvedProject) -> Result<ast::SourceFile, String> {
    use super::log::{log_event, LogEvent};

    let path_str = path.display().to_string();
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let file = match project.files.get(&canonical) {
        Some(f) => f.clone(),
        None => {
            // Only read from disk if not already in the project
            let source_text = fs::read_to_string(path).unwrap_or_default();
            parse::try_parse(&source_text)
                .map_err(|e| {
                    let msg = format!("{}: {}", path.display(), e);
                    log_event(&LogEvent::ParseError { file: &path_str, message: &e.message, source: &source_text });
                    msg
                })?
        }
    };
    let source_dir = path.parent();
    let errors = check::check_with_registry_and_dir(&file, &project.registry, source_dir);
    if !errors.is_empty() {
        // Only read source for error logging
        let source_text = fs::read_to_string(path).unwrap_or_default();
        log_event(&LogEvent::CheckErrors { file: &path_str, errors: &errors, source: &source_text });
        let msg = errors.iter().map(|e| format!("{}", e)).collect::<Vec<_>>().join("\n");
        return Err(msg);
    }
    Ok(file)
}

/// Like resolve_file_from_project but skips non-critical diagnostics.
pub fn resolve_file_from_project_lenient(path: &Path, project: &resolve::ResolvedProject) -> Result<ast::SourceFile, String> {
    use super::log::{log_event, LogEvent};
    use roca_errors as errors;

    let path_str = path.display().to_string();
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

    let file = match project.files.get(&canonical) {
        Some(f) => f.clone(),
        None => {
            let source_text = fs::read_to_string(path).unwrap_or_default();
            parse::try_parse(&source_text)
                .map_err(|e| {
                    let msg = format!("{}: {}", path.display(), e);
                    log_event(&LogEvent::ParseError { file: &path_str, message: &e.message, source: &source_text });
                    msg
                })?
        }
    };
    let source_dir = path.parent();
    let all_errors = check::check_with_registry_and_dir(&file, &project.registry, source_dir);
    let critical: Vec<_> = all_errors.iter()
        .filter(|e| e.code != errors::MISSING_DOC && e.code != errors::MISSING_TEST
                  && e.code != errors::OK_ON_INFALLIBLE && e.code != errors::RESERVED_NAME)
        .collect();
    if !critical.is_empty() {
        let source_text = fs::read_to_string(path).unwrap_or_default();
        log_event(&LogEvent::CheckErrors { file: &path_str, errors: &all_errors, source: &source_text });
        let msg = critical.iter().map(|e| format!("{}", e)).collect::<Vec<_>>().join("\n");
        return Err(msg);
    }
    Ok(file)
}

pub fn collect_roca_files(dir: &Path, files: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.file_name().map_or(false, |n| n == "out") {
                    continue;
                }
                collect_roca_files(&path, files);
            } else if path.extension().map_or(false, |e| e == "roca") {
                files.push(path);
            }
        }
    }
}

