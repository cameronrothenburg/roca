//! Check command — parses and validates `.roca` files without emitting JS.

use std::path::{Path, PathBuf};

use roca_resolve as resolve;
use super::config::*;
use super::log::{log_event, LogEvent};

pub fn check_file(path: &Path) {
    let project = resolve::resolve_file(path);
    match resolve_file_from_project(path, &project) {
        Ok(_) => {
            log_event(&LogEvent::BuildSuccess { file: &path.display().to_string(), output_path: "check" });
            println!("✓ {} passed", path.display());
        }
        Err(msg) => {
            eprintln!("{}", msg);
            std::process::exit(1);
        }
    }
}

pub fn check_directory(dir: &Path) {
    let project = resolve::resolve_directory(dir);
    let mut files: Vec<PathBuf> = Vec::new();
    collect_roca_files(dir, &mut files);

    if files.is_empty() {
        eprintln!("no .roca files found in {}", dir.display());
        std::process::exit(1);
    }

    let mut total_errors = 0;
    for file_path in &files {
        if let Err(msg) = resolve_file_from_project(file_path, &project) {
            eprintln!("{}", msg);
            total_errors += 1;
        }
    }

    if total_errors == 0 {
        println!("✓ {} file(s) checked — all passed", files.len());
    } else {
        eprintln!("\n✗ {} file(s) failed", total_errors);
        std::process::exit(1);
    }
}
