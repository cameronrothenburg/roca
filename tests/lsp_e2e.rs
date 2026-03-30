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

fn read_message(reader: &mut BufReader<impl std::io::Read>) -> String {
    let mut header = String::new();
    loop {
        header.clear();
        reader.read_line(&mut header).unwrap();
        let trimmed = header.trim();
        if trimmed.starts_with("Content-Length:") {
            let len: usize = trimmed.split(": ").nth(1).unwrap().parse().unwrap();
            // Consume blank line
            let mut blank = String::new();
            reader.read_line(&mut blank).unwrap();
            // Read body
            let mut body = vec![0u8; len];
            std::io::Read::read_exact(reader, &mut body).unwrap();
            return String::from_utf8(body).unwrap();
        }
    }
}

/// Read messages until we get one with an "id" field (response, not notification)
fn read_response(reader: &mut BufReader<impl std::io::Read>) -> String {
    loop {
        let msg = read_message(reader);
        if msg.contains(r#""id""#) {
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

    // Initialize
    send_request(&mut stdin, 1, "initialize", r#"{"capabilities":{}}"#);
    let resp = read_response(&mut stdout);
    assert!(resp.contains("capabilities"), "init response: {}", resp);

    send_notification(&mut stdin, "initialized", "{}");

    // Shutdown
    send_request(&mut stdin, 2, "shutdown", "null");
    let resp = read_response(&mut stdout);
    assert!(resp.contains(r#""id":2"#), "shutdown response: {}", resp);

    send_notification(&mut stdin, "exit", "null");
    let status = child.wait().unwrap();
    assert!(status.success());
}

#[test]
fn lsp_open_file_and_get_symbols() {
    let (mut stdin, mut stdout, mut child) = spawn_lsp();

    send_request(&mut stdin, 1, "initialize", r#"{"capabilities":{}}"#);
    let _ = read_response(&mut stdout);
    send_notification(&mut stdin, "initialized", "{}");

    // Open a file
    let content = r#"import { Http } from std::http\npub fn greet(name: String) -> String {\n    return name\n    test {}\n}"#;
    send_notification(&mut stdin, "textDocument/didOpen", &format!(
        r#"{{"textDocument":{{"uri":"file:///test.roca","languageId":"roca","version":1,"text":"{}"}}}}"#,
        content
    ));

    // Small delay for server to process
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Request symbols
    send_request(&mut stdin, 2, "textDocument/documentSymbol", r#"{"textDocument":{"uri":"file:///test.roca"}}"#);
    let resp = read_response(&mut stdout);
    assert!(resp.contains("greet"), "symbols should contain greet: {}", resp);

    // Shutdown
    send_request(&mut stdin, 3, "shutdown", "null");
    let _ = read_response(&mut stdout);
    send_notification(&mut stdin, "exit", "null");
    let _ = child.wait();
}

#[test]
fn lsp_completion_stdlib_modules() {
    let (mut stdin, mut stdout, mut child) = spawn_lsp();

    send_request(&mut stdin, 1, "initialize", r#"{"capabilities":{}}"#);
    let _ = read_response(&mut stdout);
    send_notification(&mut stdin, "initialized", "{}");

    let content = "import { Http } from std::";
    send_notification(&mut stdin, "textDocument/didOpen", &format!(
        r#"{{"textDocument":{{"uri":"file:///test.roca","languageId":"roca","version":1,"text":"{}"}}}}"#,
        content
    ));

    std::thread::sleep(std::time::Duration::from_millis(200));

    send_request(&mut stdin, 2, "textDocument/completion", &format!(
        r#"{{"textDocument":{{"uri":"file:///test.roca"}},"position":{{"line":0,"character":{}}}}}"#,
        content.len()
    ));
    let resp = read_response(&mut stdout);
    assert!(resp.contains("json") || resp.contains("http") || resp.contains("crypto"),
        "should suggest stdlib modules: {}", resp);

    send_request(&mut stdin, 3, "shutdown", "null");
    let _ = read_response(&mut stdout);
    send_notification(&mut stdin, "exit", "null");
    let _ = child.wait();
}
