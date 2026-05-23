use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const BINARY: &str = env!("CARGO_BIN_EXE_badjuju");

struct LspSession {
    child: Child,
    stdin: ChildStdin,
    reader: BufReader<ChildStdout>,
    next_id: u64,
}

impl LspSession {
    fn start(workdir: &Path) -> Self {
        let mut child = Command::new(BINARY)
            .args(["lsp"])
            .current_dir(workdir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn badjuju lsp");

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        Self {
            child,
            stdin,
            reader: BufReader::new(stdout),
            next_id: 1,
        }
    }

    fn send(&mut self, msg: serde_json::Value) {
        let body = serde_json::to_string(&msg).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn recv(&mut self) -> serde_json::Value {
        let mut content_length = 0usize;
        loop {
            let mut line = String::new();
            self.reader.read_line(&mut line).unwrap();
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(val) = trimmed.strip_prefix("Content-Length: ") {
                content_length = val.parse().unwrap();
            }
        }
        let mut body = vec![0u8; content_length];
        Read::read_exact(&mut self.reader, &mut body).unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn request(&mut self, method: &str, params: serde_json::Value) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }));
        let expected_id = serde_json::json!(id);
        loop {
            let msg = self.recv();
            if msg.get("id") == Some(&expected_id) {
                return msg;
            }
        }
    }

    fn notify(&mut self, method: &str, params: serde_json::Value) {
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }));
    }

    fn initialize(&mut self, root_uri: &str) {
        let resp = self.request(
            "initialize",
            serde_json::json!({
                "processId": null,
                "rootUri": root_uri,
                "capabilities": {}
            }),
        );
        assert!(resp.get("result").is_some(), "initialize failed: {resp}");
        self.notify("initialized", serde_json::json!({}));
    }

    fn execute_command(&mut self, command: &str) -> String {
        let resp = self.request(
            "workspace/executeCommand",
            serde_json::json!({
                "command": command,
                "arguments": [],
            }),
        );
        assert!(
            resp.get("error").is_none(),
            "{command} returned error: {resp}"
        );
        resp["result"].as_str().unwrap().to_string()
    }

    fn execute_command_with_args(
        &mut self,
        command: &str,
        arguments: serde_json::Value,
    ) -> serde_json::Value {
        self.request(
            "workspace/executeCommand",
            serde_json::json!({
                "command": command,
                "arguments": arguments,
            }),
        )
    }

    fn did_open(&mut self, uri: &str, language_id: &str, text: &str) {
        self.notify(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": text,
                }
            }),
        );
    }
}

impl Drop for LspSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn init_jj_repo(dir: &Path) {
    Command::new("jj")
        .args(["git", "init"])
        .current_dir(dir)
        .output()
        .expect("jj git init failed");
}

fn read_file(uri: &str) -> String {
    let path = uri.strip_prefix("file://").unwrap();
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn lsp_execute_status_returns_uri_with_status_header() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let uri = session.execute_command("badjuju.status");
    assert!(uri.starts_with("file://"), "unexpected URI: {uri}");
    let content = read_file(&uri);
    assert!(content.contains("STATUS:"));
    assert!(content.contains("STACK:"));
    assert!(content.contains("COMMAND REFERENCE:"));
}

#[test]
fn lsp_execute_log_returns_uri_with_revset_header() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let uri = session.execute_command("badjuju.log");
    assert!(uri.starts_with("file://"), "unexpected URI: {uri}");
    let content = read_file(&uri);
    assert!(content.contains("REVSET:"));
}

#[test]
fn lsp_execute_describe_returns_uri_with_jj_comments() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let uri = session.execute_command("badjuju.describe");
    assert!(uri.starts_with("file://"), "unexpected URI: {uri}");
    let content = read_file(&uri);
    assert!(content.contains("JJ:"));
}

#[test]
fn lsp_execute_new_creates_change_and_returns_status_uri() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let uri = session.execute_command("badjuju.new");
    assert!(uri.starts_with("file://"), "unexpected URI: {uri}");
    let content = read_file(&uri);
    assert!(content.contains("STATUS:"));
}

/// Find the 0-indexed line in `content` that starts with one of the commit
/// graph chars and parse out the change id (the first ASCII-lowercase word).
fn first_commit_line(content: &str) -> Option<(usize, String)> {
    for (i, line) in content.lines().enumerate() {
        let Some(first) = line.chars().next() else {
            continue;
        };
        if !matches!(first, '@' | '○' | '●' | '◆' | '*') {
            continue;
        }
        let rest = line[first.len_utf8()..].trim_start();
        let id: String = rest
            .chars()
            .take_while(|c| c.is_ascii_lowercase())
            .collect();
        if !id.is_empty() {
            return Some((i, id));
        }
    }
    None
}

#[test]
fn lsp_execute_describe_with_cursor_form_resolves_revision_from_log() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let log_uri = session.execute_command("badjuju.log");
    let log_content = read_file(&log_uri);
    let (line, change_id) = first_commit_line(&log_content)
        .unwrap_or_else(|| panic!("no commit line found in log buffer:\n{log_content}"));

    session.did_open(&log_uri, "jujutsu", &log_content);

    let resp = session.execute_command_with_args(
        "badjuju.describe",
        serde_json::json!([{ "cursor": { "uri": log_uri, "line": line } }]),
    );
    assert!(
        resp.get("error").is_none(),
        "describe with cursor form returned error: {resp}"
    );
    let describe_uri = resp["result"].as_str().unwrap();
    let describe_content = read_file(describe_uri);
    assert!(describe_content.contains("JJ:"));
    // describe.jujutsu should reference the resolved change id in its trailer.
    assert!(
        describe_content.contains(&change_id),
        "describe.jujutsu should mention change_id `{change_id}`:\n{describe_content}"
    );
}

#[test]
fn lsp_execute_describe_legacy_string_form_still_works() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.execute_command_with_args("badjuju.describe", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "describe with legacy string form returned error: {resp}"
    );
    let uri = resp["result"].as_str().unwrap();
    let content = read_file(uri);
    assert!(content.contains("JJ:"));
}

#[test]
fn lsp_execute_cursor_form_invalid_buffer_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.execute_command_with_args(
        "badjuju.describe",
        serde_json::json!([
            { "cursor": { "uri": "file:///nope/other.txt", "line": 0 } }
        ]),
    );
    assert!(
        resp.get("error").is_some(),
        "expected error for unsupported buffer URI, got: {resp}"
    );
}
