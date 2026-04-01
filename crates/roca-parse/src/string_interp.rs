//! String interpolation helpers — escape handling, interpolation detection, and parsing.

use roca_ast::{Expr, StringPart};
use super::expr::Parser;

/// Strip escape sequences for braces: `\{` -> `{`, `\}` -> `}`, `\\` -> `\`.
pub fn strip_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some(&'{') | Some(&'}') | Some(&'\\') => {
                    result.push(chars.next().unwrap());
                }
                _ => result.push(c),
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Count consecutive backslashes immediately before position `pos` in a char slice.
fn count_preceding_backslashes(chars: &[char], pos: usize) -> usize {
    let mut count = 0;
    let mut i = pos;
    while i > 0 {
        i -= 1;
        if chars[i] == '\\' {
            count += 1;
        } else {
            break;
        }
    }
    count
}

/// Check if a string contains interpolation expressions like {name} or {obj.field}.
/// Empty braces {} are NOT interpolation.
/// Content with non-identifier characters (colons, commas, spaces) is NOT interpolation.
/// Only {identifier} and {obj.field} patterns count as interpolation.
pub fn has_interpolation(s: &str) -> bool {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' && count_preceding_backslashes(&chars, i) % 2 == 0 {
            let start = i + 1;
            i += 1;
            let mut found_close = false;
            while i < chars.len() {
                if chars[i] == '}' {
                    found_close = true;
                    break;
                }
                i += 1;
            }
            if !found_close { continue; }
            let content: String = chars[start..i].iter().collect();
            i += 1; // skip '}'
            let trimmed = content.trim();
            if trimmed.is_empty() { continue; }
            // Must start with a letter or underscore (not a digit)
            let first = trimmed.chars().next().unwrap();
            if !first.is_alphabetic() && first != '_' { continue; }
            // Only valid interpolation if content is an identifier path or method call
            // Allows: {name}, {user.age}, {value.toString()}, {item.toLog()}
            if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '(' || c == ')') {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// Parse "hello {name}, age {age}" into StringInterp parts.
/// Escaped braces `\{` and `\}` are treated as literal `{` and `}`.
/// `\\` before a brace is a literal backslash (the brace starts interpolation).
pub fn parse_string_interp(s: &str) -> Expr {
    let chars: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            if next == '{' || next == '}' {
                // \{ or \} -> literal brace
                current.push(next);
                i += 2;
                continue;
            }
            if next == '\\' {
                // \\ -> literal backslash
                current.push('\\');
                i += 2;
                continue;
            }
            current.push(chars[i]);
            i += 1;
            continue;
        }
        if chars[i] == '{' {
            if !current.is_empty() {
                parts.push(StringPart::Literal(current.clone()));
                current.clear();
            }
            i += 1; // skip '{'
            let mut expr_str = String::new();
            while i < chars.len() && chars[i] != '}' {
                expr_str.push(chars[i]);
                i += 1;
            }
            if i < chars.len() { i += 1; } // skip '}'

            let trimmed = expr_str.trim();
            if trimmed.contains('.') {
                let tokens = crate::tokenize(trimmed);
                let mut p = Parser::new(tokens);
                parts.push(StringPart::Expr(p.parse_expr().unwrap()));
            } else {
                parts.push(StringPart::Expr(Expr::Ident(trimmed.to_string())));
            }
        } else {
            current.push(chars[i]);
            i += 1;
        }
    }
    if !current.is_empty() {
        parts.push(StringPart::Literal(current));
    }

    Expr::StringInterp(parts)
}
