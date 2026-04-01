//! Structured event logging — writes build, test, and error events to `roca.jsonl`.

use std::fs;
use std::path::Path;
use std::io::Write;

pub fn log_event(event: &LogEvent) {
    if !tracking_enabled() { return; }

    let log_dir = match dirs() {
        Some(d) => d,
        None => return,
    };

    let _ = fs::create_dir_all(&log_dir);
    let log_path = log_dir.join("roca.jsonl");

    let json = match event {
        LogEvent::ParseError { file, message, source } => {
            format!(r#"{{"ts":"{}","event":"parse_error","file":"{}","message":"{}","source":"{}"}}"#,
                timestamp(), escape(file), escape(message), escape(source))
        }
        LogEvent::CheckErrors { file, errors, source } => {
            let errs: Vec<String> = errors.iter().map(|e| {
                format!(r#"{{"code":"{}","message":"{}","context":{}}}"#,
                    escape(&e.code), escape(&e.message),
                    e.context.as_ref().map(|c| format!("\"{}\"", escape(c))).unwrap_or("null".into()))
            }).collect();
            format!(r#"{{"ts":"{}","event":"check_errors","file":"{}","error_count":{},"errors":[{}],"source":"{}"}}"#,
                timestamp(), escape(file), errors.len(), errs.join(","), escape(source))
        }
        LogEvent::BuildSuccess { file, output_path } => {
            format!(r#"{{"ts":"{}","event":"build_success","file":"{}","output":"{}"}}"#,
                timestamp(), escape(file), escape(output_path))
        }
        LogEvent::BuildFailed { file, reason } => {
            format!(r#"{{"ts":"{}","event":"build_failed","file":"{}","reason":"{}"}}"#,
                timestamp(), escape(file), escape(reason))
        }
    };

    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&log_path) {
        let _ = writeln!(f, "{}", json);
    }
}

pub enum LogEvent<'a> {
    ParseError {
        file: &'a str,
        message: &'a str,
        source: &'a str,
    },
    CheckErrors {
        file: &'a str,
        errors: &'a [crate::errors::RuleError],
        source: &'a str,
    },
    BuildSuccess {
        file: &'a str,
        output_path: &'a str,
    },
    BuildFailed {
        file: &'a str,
        reason: &'a str,
    },
}

fn dirs() -> Option<std::path::PathBuf> {
    std::env::var("HOME").ok().map(|h| Path::new(&h).join(".roca").join("logs"))
}

fn timestamp() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", dur.as_secs())
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

/// Check roca.toml for tracking = false. Walks up from cwd.
fn tracking_enabled() -> bool {
    let mut dir = std::env::current_dir().unwrap_or_default();
    loop {
        let config = dir.join("roca.toml");
        if config.exists() {
            if let Ok(content) = fs::read_to_string(&config) {
                for line in content.lines() {
                    let t = line.trim();
                    if t.starts_with("tracking") && t.contains('=') {
                        if let Some(val) = t.split('=').nth(1) {
                            return val.trim().trim_matches('"') != "false";
                        }
                    }
                }
            }
            return true; // roca.toml found, no tracking key — default on
        }
        match dir.parent() {
            Some(p) if p != dir => dir = p.to_path_buf(),
            _ => return true, // no roca.toml found — default on
        }
    }
}
