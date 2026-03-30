//! E2E LSP tests — spawn the roca lsp server and communicate via JSON-RPC.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

fn send(stdin: &mut impl Write, body: &str) {
    let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    stdin.write_all(msg.as_bytes()).unwrap();
    stdin.flush().unwrap();
}

fn send_request(stdin: &mut impl Write, id: u32, method: &str, params: &str) {
    send(stdin, &format!(
        r#"{{"jsonrpc":"2.0","id":{},"method":"{}","params":{}}}"#,
        id, method, params
    ));
}

fn send_notification(stdin: &mut impl Write, method: &str, params: &str) {
    send(stdin, &format!(
        r#"{{"jsonrpc":"2.0","method":"{}","params":{}}}"#,
        method, params
    ));
}

fn read_message(reader: &mut BufReader<std::process::ChildStdout>) -> String {
    let mut header = String::new();
    loop {
        header.clear();
        if reader.read_line(&mut header).unwrap() == 0 {
            return String::new(); // EOF
        }
        let trimmed = header.trim();
        if trimmed.starts_with("Content-Length:") {
            let len: usize = trimmed.split(": ").nth(1).unwrap().parse().unwrap();
            let mut blank = String::new();
            reader.read_line(&mut blank).unwrap();
            let mut body = vec![0u8; len];
            std::io::Read::read_exact(reader, &mut body).unwrap();
            return String::from_utf8(body).unwrap();
        }
    }
}

/// Read messages until we get a response (has "result" or "error" with an "id")
fn read_response(reader: &mut BufReader<std::process::ChildStdout>) -> String {
    loop {
        let msg = read_message(reader);
        if msg.is_empty() { return msg; }
        // Notifications have "method" but no "id" at the top level
        // Responses have "id" and either "result" or "error"
        if msg.contains(r#""result""#) || (msg.contains(r#""error""#) && msg.contains(r#""id""#)) {
            return msg;
        }
    }
}

fn spawn_lsp() -> (std::process::ChildStdin, BufReader<std::process::ChildStdout>, std::process::Child) {
    let exe = env!("CARGO_BIN_EXE_roca");
    let mut child = Command::new(exe)
        .arg("lsp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn roca lsp");

    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    (stdin, stdout, child)
}

// ─── Tests ─────────────────────────────────────────────

#[test]
fn lsp_initialize_and_shutdown() {
    let (mut stdin, mut stdout, mut child) = spawn_lsp();

    send_request(&mut stdin, 1, "initialize", r#"{"capabilities":{}}"#);
    let resp = read_response(&mut stdout);
    assert!(resp.contains("capabilities"), "init should return capabilities: {}", resp);
    assert!(resp.contains("completionProvider"), "should support completion: {}", resp);

    send_notification(&mut stdin, "initialized", "{}");

    send_request(&mut stdin, 2, "shutdown", "null");
    let resp = read_response(&mut stdout);
    assert!(resp.contains(r#""id":2"#), "shutdown response: {}", resp);

    send_notification(&mut stdin, "exit", "null");
    drop(stdin);
    let status = child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn lsp_document_symbols() {
    let (mut stdin, mut stdout, mut child) = spawn_lsp();

    send_request(&mut stdin, 1, "initialize", r#"{"capabilities":{}}"#);
    let _ = read_response(&mut stdout);
    send_notification(&mut stdin, "initialized", "{}");

    send_notification(&mut stdin, "textDocument/didOpen", &format!(
        r#"{{"textDocument":{{"uri":"file:///test.roca","languageId":"roca","version":1,"text":"pub fn greet(name: String) -> String {{\n    return name\n    test {{}}\n}}"}}}}"#
    ));

    // Wait for diagnostics to be published
    std::thread::sleep(std::time::Duration::from_millis(200));

    send_request(&mut stdin, 2, "textDocument/documentSymbol",
        r#"{"textDocument":{"uri":"file:///test.roca"}}"#);
    let resp = read_response(&mut stdout);
    assert!(resp.contains("greet"), "should contain function symbol: {}", resp);

    send_request(&mut stdin, 3, "shutdown", "null");
    let _ = read_response(&mut stdout);
    send_notification(&mut stdin, "exit", "null");
    drop(stdin);
    let _ = child.wait();
}

#[test]
fn lsp_completion_stdlib_modules() {
    let (mut stdin, mut stdout, mut child) = spawn_lsp();

    send_request(&mut stdin, 1, "initialize", r#"{"capabilities":{}}"#);
    let _ = read_response(&mut stdout);
    send_notification(&mut stdin, "initialized", "{}");

    send_notification(&mut stdin, "textDocument/didOpen", &format!(
        r#"{{"textDocument":{{"uri":"file:///test.roca","languageId":"roca","version":1,"text":"import {{ Http }} from std::"}}}}"#
    ));

    std::thread::sleep(std::time::Duration::from_millis(200));

    send_request(&mut stdin, 2, "textDocument/completion", &format!(
        r#"{{"textDocument":{{"uri":"file:///test.roca"}},"position":{{"line":0,"character":27}}}}"#
    ));
    let resp = read_response(&mut stdout);
    assert!(resp.contains("json") || resp.contains("http") || resp.contains("crypto"),
        "should suggest stdlib modules: {}", resp);

    send_request(&mut stdin, 3, "shutdown", "null");
    let _ = read_response(&mut stdout);
    send_notification(&mut stdin, "exit", "null");
    drop(stdin);
    let _ = child.wait();
}
