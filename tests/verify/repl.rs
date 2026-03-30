//! REPL integration tests — verify the REPL produces correct output.

use std::process::{Command, Stdio};
use std::io::Write;

fn repl(input: &str) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_roca"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to start roca repl");

    child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn repl_output(input: &str) -> Vec<String> {
    let raw = repl(&format!("{}\n:q\n", input));
    let mut results = Vec::new();
    for line in raw.lines() {
        if line.starts_with("Roca REPL") || line.starts_with("Type Roca") || line.is_empty() || line == "bye" {
            continue;
        }
        // Extract value after "roca> " prompt
        if line.starts_with("roca> ") {
            let val = line.strip_prefix("roca> ").unwrap().trim();
            if !val.is_empty() {
                results.push(val.to_string());
            }
            continue;
        }
        // Multi-line continuation that has output
        if line.starts_with("  ... ") {
            let val = line.strip_prefix("  ... ").unwrap().trim();
            if !val.is_empty() {
                results.push(val.to_string());
            }
            continue;
        }
        // Standalone output lines (errors, etc.)
        results.push(line.trim().to_string());
    }
    results
}

#[test]
fn arithmetic() {
    let out = repl_output("1 + 2");
    assert!(out.iter().any(|l| l.contains("3")), "expected 3, got: {:?}", out);
}

#[test]
fn string_method() {
    let out = repl_output("\"hello\".toUpperCase()");
    assert!(out.iter().any(|l| l.contains("HELLO")), "expected HELLO, got: {:?}", out);
}

#[test]
fn boolean_logic() {
    let out = repl_output("true && false");
    assert!(out.iter().any(|l| l.contains("false")), "expected false, got: {:?}", out);
}

#[test]
fn string_concat() {
    let out = repl_output("\"hello \" + \"world\"");
    assert!(out.iter().any(|l| l.contains("hello world")), "expected hello world, got: {:?}", out);
}

#[test]
fn define_and_call_function() {
    let input = "fn add(a: Number, b: Number) -> Number {\n    return a + b\n    test { self(1, 2) == 3 }\n}\nadd(10, 20)";
    let out = repl_output(input);
    assert!(out.iter().any(|l| l.contains("defined")), "expected defined, got: {:?}", out);
    assert!(out.iter().any(|l| l.contains("30")), "expected 30, got: {:?}", out);
}

#[test]
fn struct_as_json() {
    let input = "pub struct Point { x: Number y: Number }{}\nPoint { x: 5, y: 10 }";
    let out = repl_output(input);
    assert!(out.iter().any(|l| l.contains("defined")), "expected defined, got: {:?}", out);
    assert!(out.iter().any(|l| l.contains("\"x\":5") || l.contains("\"x\": 5")),
        "expected JSON struct, got: {:?}", out);
}

#[test]
fn array_join() {
    let out = repl_output("[1, 2, 3].join(\"-\")");
    assert!(out.iter().any(|l| l.contains("1-2-3")), "expected 1-2-3, got: {:?}", out);
}

#[test]
fn compiler_error_shown() {
    // Calling an error-returning function without crash block should show error
    let input = "fn bad() -> String, err {\n    err fail = \"fail\"\n    return \"ok\"\n    test { self() == \"ok\" }\n}\nbad()";
    let out = repl_output(input);
    assert!(out.iter().any(|l| l.contains("missing-crash") || l.contains("crash")),
        "expected compiler error, got: {:?}", out);
}
