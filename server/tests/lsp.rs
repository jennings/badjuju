use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_badjuju");

struct LspSession {
    child: Child,
    stdin: ChildStdin,
    msg_rx: Receiver<serde_json::Value>,
    _reader_thread: JoinHandle<()>,
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
        let (tx, rx) = channel();
        let reader_thread = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut content_length = 0usize;
                let mut got_header = false;
                loop {
                    let mut line = String::new();
                    match reader.read_line(&mut line) {
                        Ok(0) => return,
                        Ok(_) => {}
                        Err(_) => return,
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        if got_header {
                            break;
                        } else {
                            continue;
                        }
                    }
                    got_header = true;
                    if let Some(val) = trimmed.strip_prefix("Content-Length: ") {
                        content_length = val.parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; content_length];
                if Read::read_exact(&mut reader, &mut body).is_err() {
                    return;
                }
                let Ok(msg) = serde_json::from_slice::<serde_json::Value>(&body) else {
                    return;
                };
                if tx.send(msg).is_err() {
                    return;
                }
            }
        });

        Self {
            child,
            stdin,
            msg_rx: rx,
            _reader_thread: reader_thread,
            next_id: 1,
        }
    }

    fn send(&mut self, msg: serde_json::Value) {
        let body = serde_json::to_string(&msg).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", body.len(), body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn recv(&mut self) -> serde_json::Value {
        self.msg_rx.recv().expect("LSP session disconnected")
    }

    fn try_recv(&mut self, timeout: Duration) -> Option<serde_json::Value> {
        self.msg_rx.recv_timeout(timeout).ok()
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

    fn initialize(&mut self, root_uri: &str) -> serde_json::Value {
        self.initialize_with_options(root_uri, serde_json::Value::Null)
    }

    fn initialize_with_options(
        &mut self,
        root_uri: &str,
        initialization_options: serde_json::Value,
    ) -> serde_json::Value {
        let mut params = serde_json::json!({
            "processId": null,
            "rootUri": root_uri,
            "capabilities": {}
        });
        if !initialization_options.is_null() {
            params["initializationOptions"] = initialization_options;
        }
        let resp = self.request("initialize", params);
        assert!(resp.get("result").is_some(), "initialize failed: {resp}");
        self.notify("initialized", serde_json::json!({}));
        resp
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

    /// Like `execute_command_with_args`, but automatically responds to any
    /// `workspace/applyEdit` server requests received while waiting for the
    /// command response. Required for squash commands that call `apply_edit`
    /// before returning — without this the exchange deadlocks.
    fn execute_command_acked(
        &mut self,
        command: &str,
        arguments: serde_json::Value,
    ) -> serde_json::Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "workspace/executeCommand",
            "params": {
                "command": command,
                "arguments": arguments,
            },
        }));
        let expected_id = serde_json::json!(id);
        loop {
            let msg = self.recv();
            if msg.get("id") == Some(&expected_id) {
                return msg;
            }
            // Auto-ACK workspace/applyEdit so the server's apply_edit() doesn't block.
            if msg.get("method").and_then(|v| v.as_str()) == Some("workspace/applyEdit") {
                if let Some(req_id) = msg.get("id").cloned() {
                    self.send(serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": req_id,
                        "result": { "applied": true }
                    }));
                }
            }
        }
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

    /// Read messages from the server until a server-initiated request with the given method
    /// is found. Notifications and requests with other methods are silently skipped.
    fn recv_server_request(&mut self, method: &str) -> serde_json::Value {
        loop {
            let msg = self.recv();
            if msg.get("method").and_then(|v| v.as_str()) == Some(method) {
                return msg;
            }
        }
    }

    /// Read messages until a server-initiated request with the given method is
    /// found or the timeout expires. Returns `None` on timeout.
    fn recv_server_request_timeout(
        &mut self,
        method: &str,
        timeout: Duration,
    ) -> Option<serde_json::Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let remaining = deadline.checked_duration_since(Instant::now())?;
            let msg = self.try_recv(remaining)?;
            if msg.get("method").and_then(|v| v.as_str()) == Some(method) {
                return Some(msg);
            }
        }
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
    assert!(content.contains("@  :"));
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
    assert!(content.contains("@  :"));
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
fn lsp_execute_describe_string_arg_succeeds() {
    // Revision-scoped commands accept a literal string arg so Neovim CLI user
    // commands (`:JJDescribe @-`) and pre-resolved commit-line code actions
    // can pass the revision directly.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.execute_command_with_args("badjuju.describe", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "describe with string arg returned error: {resp}"
    );
    let uri = resp["result"].as_str().unwrap();
    let content = read_file(uri);
    assert!(content.contains("JJ:"));
}

#[test]
fn lsp_execute_edit_string_arg_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    // Need a non-@ commit to edit.
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "second"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.execute_command_with_args("badjuju.edit", serde_json::json!(["@-"]));
    assert!(
        resp.get("error").is_none(),
        "edit with string arg returned error: {resp}"
    );
    let uri = resp["result"].as_str().unwrap();
    assert!(uri.starts_with("file://"));
}

#[test]
fn lsp_execute_abandon_string_arg_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "second"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.execute_command_with_args("badjuju.abandon", serde_json::json!(["@-"]));
    assert!(
        resp.get("error").is_none(),
        "abandon with string arg returned error: {resp}"
    );
}

#[test]
fn lsp_execute_diff_string_arg_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.execute_command_with_args("badjuju.diff", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "diff with string arg returned error: {resp}"
    );
    let uri = resp["result"].as_str().unwrap();
    assert!(uri.starts_with("file://"));
}

#[test]
fn lsp_execute_new_string_arg_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.execute_command_with_args("badjuju.new", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "new with string arg returned error: {resp}"
    );
    let uri = resp["result"].as_str().unwrap();
    let content = read_file(uri);
    assert!(content.contains("@  :"));
}

#[test]
fn lsp_execute_rebase_string_args_succeed() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    // Three commits so `@-` rebases onto `@--` non-trivially.
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "second"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");
    Command::new("jj")
        .args(["new", "-m", "third"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp =
        session.execute_command_with_args("badjuju.rebase", serde_json::json!(["@-", "@--"]));
    assert!(
        resp.get("error").is_none(),
        "rebase with string args returned error: {resp}"
    );
}

#[test]
fn lsp_execute_bookmark_string_args_succeed() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.execute_command_with_args(
        "badjuju.bookmark",
        serde_json::json!(["create", "scratch", "@"]),
    );
    assert!(
        resp.get("error").is_none(),
        "bookmark create with string args returned error: {resp}"
    );
}

/// Find the 0-indexed line in a status.jujutsu buffer that matches a status
/// header line (`M path` etc.) for the given filename.
fn status_file_line(content: &str, filename: &str) -> Option<usize> {
    for (i, line) in content.lines().enumerate() {
        // Old STATUS section bare form: "M readme.txt"
        if matches!(line.chars().next(), Some('M' | 'A' | 'D' | 'C' | 'R')) {
            if line[1..].trim() == filename {
                return Some(i);
            }
        }
        // New STACK stat line form from jj log --stat: "│  readme.txt | N +"
        if line.contains(filename) && line.contains(" | ") {
            return Some(i);
        }
    }
    None
}

#[test]
fn lsp_execute_squash_with_cursor_form_resolves_file_and_revision() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    // Set up: parent has readme.txt, working copy modifies it.
    std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "parent"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");
    std::fs::write(dir.path().join("readme.txt"), "v2\n").unwrap();

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    let line = status_file_line(&status_content, "readme.txt")
        .unwrap_or_else(|| panic!("readme.txt not found in status:\n{status_content}"));

    session.did_open(&status_uri, "jujutsu", &status_content);

    let resp = session.execute_command_with_args(
        "badjuju.squash",
        serde_json::json!([{ "cursor": { "uri": status_uri, "line": line } }]),
    );
    assert!(
        resp.get("error").is_none(),
        "squash with cursor form returned error: {resp}"
    );
    let new_status_uri = resp["result"].as_str().unwrap();
    let new_status = read_file(new_status_uri);
    assert!(new_status.contains("@  :"));
    assert!(
        !new_status.contains("M readme.txt"),
        "expected readme.txt squashed away:\n{new_status}"
    );
}

#[test]
fn lsp_execute_squash_string_args_succeed() {
    // squash accepts the legacy `[file_str, revision_str]` form so Neovim
    // CLI users can run `:JJSquash <file> @`. Setup: parent has readme.txt,
    // working copy modifies it; squash @ → parent moves the change away.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "parent"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");
    std::fs::write(dir.path().join("readme.txt"), "v2\n").unwrap();

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp =
        session.execute_command_with_args("badjuju.squash", serde_json::json!(["readme.txt", "@"]));
    assert!(
        resp.get("error").is_none(),
        "squash with string args returned error: {resp}"
    );
    let new_status_uri = resp["result"].as_str().unwrap();
    let new_status = read_file(new_status_uri);
    assert!(new_status.contains("@  :"));
    assert!(
        !new_status.contains("M readme.txt"),
        "expected readme.txt squashed away:\n{new_status}"
    );
}

#[test]
fn lsp_execute_unsquash_string_args_succeed() {
    // unsquash @- moves a file from the parent down into @ (its only child).
    // Setup: parent has readme.txt, @ is an empty child of parent.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "parent"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session
        .execute_command_with_args("badjuju.unsquash", serde_json::json!(["readme.txt", "@-"]));
    assert!(
        resp.get("error").is_none(),
        "unsquash with string args returned error: {resp}"
    );
    let new_status_uri = resp["result"].as_str().unwrap();
    let new_status = read_file(new_status_uri);
    assert!(new_status.contains("@  :"));
    assert!(
        new_status.contains("readme.txt"),
        "expected readme.txt in working copy after unsquash:\n{new_status}"
    );
}

/// Find the 0-indexed line of the first `JJ: <Label>: <revset>` shortcut line
/// in the log buffer, returning (line, revset).
fn first_log_shortcut(content: &str) -> Option<(usize, String)> {
    for (i, line) in content.lines().enumerate() {
        if !line.starts_with("JJ:") {
            continue;
        }
        let after = line.strip_prefix("JJ:")?.trim_start();
        let colon = after.find(':')?;
        let revset = after[colon + 1..].trim();
        if !revset.is_empty() {
            return Some((i, revset.to_string()));
        }
    }
    None
}

#[test]
fn lsp_execute_log_with_cursor_form_on_shortcut_applies_revset() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let log_uri = session.execute_command("badjuju.log");
    let log_content = read_file(&log_uri);
    let (line, expected_revset) = first_log_shortcut(&log_content)
        .unwrap_or_else(|| panic!("no log shortcut line found:\n{log_content}"));

    session.did_open(&log_uri, "jujutsu", &log_content);

    let resp = session.execute_command_with_args(
        "badjuju.log",
        serde_json::json!([{ "cursor": { "uri": log_uri, "line": line } }]),
    );
    assert!(
        resp.get("error").is_none(),
        "log with cursor form returned error: {resp}"
    );
    let new_log_uri = resp["result"].as_str().unwrap();
    let new_log = read_file(new_log_uri);
    assert!(
        new_log.starts_with(&format!("REVSET: {expected_revset}")),
        "expected REVSET header `{expected_revset}`, got:\n{new_log}"
    );
}

#[test]
fn lsp_execute_log_with_cursor_form_on_non_shortcut_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let log_uri = session.execute_command("badjuju.log");
    let log_content = read_file(&log_uri);
    // The REVSET: line itself (line 0) isn't a shortcut.
    session.did_open(&log_uri, "jujutsu", &log_content);

    let resp = session.execute_command_with_args(
        "badjuju.log",
        serde_json::json!([{ "cursor": { "uri": log_uri, "line": 0 } }]),
    );
    assert!(
        resp.get("error").is_some(),
        "expected error for non-shortcut line, got: {resp}"
    );
}

#[test]
fn lsp_execute_log_legacy_string_form_still_works() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.execute_command_with_args("badjuju.log", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "log with legacy form returned error: {resp}"
    );
    let uri = resp["result"].as_str().unwrap();
    let content = read_file(uri);
    assert!(content.starts_with("REVSET: @"));
}

#[test]
fn lsp_advertises_code_action_provider_capability() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    let init = session.initialize(&root_uri);
    let cap = &init["result"]["capabilities"]["codeActionProvider"];
    assert!(
        !cap.is_null(),
        "expected codeActionProvider in capabilities, got: {init}"
    );
}

#[test]
fn lsp_code_action_returns_empty_list() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let log_uri = session.execute_command("badjuju.log");
    let resp = session.request(
        "textDocument/codeAction",
        serde_json::json!({
            "textDocument": { "uri": log_uri },
            "range": {
                "start": { "line": 0, "character": 0 },
                "end":   { "line": 0, "character": 0 }
            },
            "context": { "diagnostics": [] }
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "code action returned error: {resp}"
    );
    let result = &resp["result"];
    assert!(result.is_array(), "expected array, got: {result}");
    assert_eq!(result.as_array().unwrap().len(), 0);
}

/// Send a `textDocument/codeAction` for `uri` at `line` and return the array
/// of returned `CodeAction` objects (or empty array on no actions).
fn code_action_at(session: &mut LspSession, uri: &str, line: usize) -> Vec<serde_json::Value> {
    let resp = session.request(
        "textDocument/codeAction",
        serde_json::json!({
            "textDocument": { "uri": uri },
            "range": {
                "start": { "line": line, "character": 0 },
                "end":   { "line": line, "character": 0 }
            },
            "context": { "diagnostics": [] }
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "code action returned error: {resp}"
    );
    resp["result"].as_array().cloned().unwrap_or_default()
}

#[test]
fn lsp_code_action_on_log_commit_line_returns_seven_actions() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    // Create a second commit so the log buffer has at least two commit lines.
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "second"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let log_uri = session.execute_command("badjuju.log");
    let log_content = read_file(&log_uri);
    let (commit_line, change_id) =
        first_commit_line(&log_content).unwrap_or_else(|| panic!("no commit line:\n{log_content}"));

    session.did_open(&log_uri, "jujutsu", &log_content);

    let actions = code_action_at(&mut session, &log_uri, commit_line);
    let expected: &[(&str, &str)] = &[
        ("Edit commit", "badjuju.edit"),
        ("Abandon commit", "badjuju.abandon"),
        ("Describe commit", "badjuju.describe"),
        ("Show diff for", "badjuju.diff"),
        ("New child of", "badjuju.new"),
        ("Rebase commit", "badjuju.client.rebasePrompt"),
        ("Bookmark", "badjuju.client.bookmarkPrompt"),
    ];
    // 7 standard commit actions + 1 squash-flow action
    assert_eq!(
        actions.len(),
        expected.len() + 1,
        "expected {} actions (7 commit + 1 squash), got {}: {actions:?}",
        expected.len() + 1,
        actions.len()
    );
    for (action, (title_prefix, command_name)) in actions.iter().zip(expected) {
        let title = action["title"].as_str().expect("missing title");
        assert!(
            title.starts_with(title_prefix),
            "title `{title}` should start with `{title_prefix}`"
        );
        assert!(
            title.contains(&change_id),
            "title `{title}` should contain change_id `{change_id}`"
        );
        let cmd = &action["command"];
        assert_eq!(cmd["command"].as_str(), Some(*command_name));
        let args = cmd["arguments"].as_array().expect("missing arguments");
        assert_eq!(args.len(), 1, "expected one argument");
        assert_eq!(args[0].as_str(), Some(change_id.as_str()));
    }
    // Verify squash action is present
    let squash_action = &actions[7];
    assert_eq!(
        squash_action["title"].as_str(),
        Some("Squash from this revision")
    );
    assert_eq!(
        squash_action["command"]["command"].as_str(),
        Some("badjuju.squash.commit")
    );

    // Direct-action commands ship a Value::String(revision); the server must
    // accept that string-form so picking the action actually invokes the
    // command rather than erroring out. Invoke `badjuju.describe` since it
    // doesn't mutate the repo.
    let describe_action = actions
        .iter()
        .find(|a| a["command"]["command"].as_str() == Some("badjuju.describe"))
        .expect("describe action missing");
    let resp = session.execute_command_with_args(
        "badjuju.describe",
        describe_action["command"]["arguments"].clone(),
    );
    assert!(
        resp.get("error").is_none(),
        "invoking describe with code-action arg returned error: {resp}"
    );
}

#[test]
fn lsp_code_action_on_status_file_line_returns_squash_unsquash() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    // Set up: parent has readme.txt, working copy modifies it.
    std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "parent"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");
    std::fs::write(dir.path().join("readme.txt"), "v2\n").unwrap();

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    let line = status_file_line(&status_content, "readme.txt")
        .unwrap_or_else(|| panic!("readme.txt not found:\n{status_content}"));
    session.did_open(&status_uri, "jujutsu", &status_content);

    let actions = code_action_at(&mut session, &status_uri, line);
    let expected: &[(&str, &str)] = &[
        ("Squash readme.txt", "badjuju.squash"),
        ("Unsquash readme.txt", "badjuju.unsquash"),
    ];
    assert_eq!(
        actions.len(),
        expected.len(),
        "expected {} actions, got {}: {actions:?}",
        expected.len(),
        actions.len()
    );
    for (action, (title, command_name)) in actions.iter().zip(expected) {
        assert_eq!(action["title"].as_str(), Some(*title));
        let cmd = &action["command"];
        assert_eq!(cmd["command"].as_str(), Some(*command_name));
        let args = cmd["arguments"].as_array().expect("missing arguments");
        assert_eq!(args.len(), 1);
        let cursor = &args[0]["cursor"];
        assert_eq!(cursor["uri"].as_str(), Some(status_uri.as_str()));
        assert_eq!(cursor["line"].as_u64(), Some(line as u64));
    }
}

#[test]
fn lsp_code_action_on_status_commit_header_returns_commit_actions() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    // Need a second commit so the status log section has a non-@ commit too.
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "second"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    let (commit_line, change_id) = first_commit_line(&status_content)
        .unwrap_or_else(|| panic!("no commit line:\n{status_content}"));
    session.did_open(&status_uri, "jujutsu", &status_content);

    let actions = code_action_at(&mut session, &status_uri, commit_line);
    // 7 standard commit actions + 1 squash-flow action
    assert_eq!(
        actions.len(),
        8,
        "expected 8 commit actions (7 standard + 1 squash), got: {actions:?}"
    );
    let first = &actions[0];
    assert!(
        first["title"]
            .as_str()
            .is_some_and(|t| t.starts_with("Edit commit") && t.contains(&change_id))
    );
    assert_eq!(first["command"]["command"].as_str(), Some("badjuju.edit"));
    let args = first["command"]["arguments"].as_array().unwrap();
    assert_eq!(args[0].as_str(), Some(change_id.as_str()));
    // Verify squash action is present at position 7
    let squash = &actions[7];
    assert_eq!(squash["title"].as_str(), Some("Squash from this revision"));
    assert_eq!(
        squash["command"]["command"].as_str(),
        Some("badjuju.squash.commit")
    );
}

#[test]
fn lsp_code_action_on_status_blank_line_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    session.did_open(&status_uri, "jujutsu", &status_content);

    // Find a blank line in the buffer (between the @/@- headers and STACK:).
    let blank_line = status_content
        .lines()
        .enumerate()
        .find_map(|(i, l)| l.is_empty().then_some(i))
        .expect("status buffer should contain a blank line");
    let actions = code_action_at(&mut session, &status_uri, blank_line);
    assert!(
        actions.is_empty(),
        "expected no actions on blank line, got: {actions:?}"
    );
}

#[test]
fn lsp_code_action_on_log_non_commit_line_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let log_uri = session.execute_command("badjuju.log");
    let log_content = read_file(&log_uri);
    session.did_open(&log_uri, "jujutsu", &log_content);

    // Line 0 is the REVSET: header — not a commit, no actions.
    let actions = code_action_at(&mut session, &log_uri, 0);
    assert!(
        actions.is_empty(),
        "expected no actions on REVSET line, got: {actions:?}"
    );
}

/// Find the 0-indexed line + label of the first `JJ: <Label>: <revset>`
/// shortcut line in the log buffer.
fn first_log_shortcut_with_label(content: &str) -> Option<(usize, String)> {
    for (i, line) in content.lines().enumerate() {
        let Some(rest) = line.strip_prefix("JJ:") else {
            continue;
        };
        let after = rest.trim_start();
        let Some(colon) = after.find(':') else {
            continue;
        };
        let label = after[..colon].trim();
        let revset = after[colon + 1..].trim();
        if !revset.is_empty() && !label.is_empty() {
            return Some((i, label.to_string()));
        }
    }
    None
}

#[test]
fn lsp_code_action_on_log_shortcut_line_returns_apply_revset() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let log_uri = session.execute_command("badjuju.log");
    let log_content = read_file(&log_uri);
    let (shortcut_line, label) = first_log_shortcut_with_label(&log_content)
        .unwrap_or_else(|| panic!("no log shortcut line:\n{log_content}"));

    session.did_open(&log_uri, "jujutsu", &log_content);

    let actions = code_action_at(&mut session, &log_uri, shortcut_line);
    assert_eq!(actions.len(), 1, "expected one action, got: {actions:?}");
    let action = &actions[0];
    let title = action["title"].as_str().expect("missing title");
    assert!(
        title.starts_with("Apply revset:") && title.contains(&label),
        "title `{title}` should mention label `{label}`"
    );
    let cmd = &action["command"];
    assert_eq!(cmd["command"].as_str(), Some("badjuju.log"));
    let args = cmd["arguments"].as_array().expect("missing arguments");
    assert_eq!(args.len(), 1);
    let cursor = &args[0]["cursor"];
    assert_eq!(cursor["uri"].as_str(), Some(log_uri.as_str()));
    assert_eq!(cursor["line"].as_u64(), Some(shortcut_line as u64));
}

#[test]
fn lsp_advertises_semantic_tokens_provider_with_legend() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    let init = session.initialize(&root_uri);
    let cap = &init["result"]["capabilities"]["semanticTokensProvider"];
    assert!(
        !cap.is_null(),
        "expected semanticTokensProvider, got: {init}"
    );
    let types = cap["legend"]["tokenTypes"]
        .as_array()
        .expect("tokenTypes array");
    let type_names: Vec<&str> = types.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(
        type_names,
        vec![
            "comment",
            "keyword",
            "string",
            "type",
            "enumMember",
            "number",
            "operator"
        ]
    );
    let mods = cap["legend"]["tokenModifiers"]
        .as_array()
        .expect("tokenModifiers array");
    let mod_names: Vec<&str> = mods.iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(mod_names, vec!["documentation"]);
    assert_eq!(cap["full"].as_bool(), Some(true));
}

#[test]
fn lsp_semantic_tokens_full_returns_expected_tokens() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    // didOpen a synthetic status.jujutsu so the assertions stay deterministic.
    let uri = format!("file://{}/status.jujutsu", dir.path().display());
    let content = "STATUS:\n\nM src/main.rs\n\n@  qpvuntsm 1234abcd\n";
    session.did_open(&uri, "jujutsu", content);

    let resp = session.request(
        "textDocument/semanticTokens/full",
        serde_json::json!({
            "textDocument": { "uri": uri }
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "semanticTokens/full returned error: {resp}"
    );
    let data = resp["result"]["data"]
        .as_array()
        .expect("data array")
        .iter()
        .map(|v| v.as_u64().unwrap() as u32)
        .collect::<Vec<_>>();
    // Layout (delta-line, delta-start, length, type, mod):
    //   STATUS:           line 0, col 0, len 7, type=1 (keyword)
    //   M (file_status)   line 2, col 0, len 2, type=3 (type)
    //   @ (graph)         line 4, col 0, len 1, type=6 (operator)
    //   qpvuntsm (chgid)  line 4, col 3, len 8, type=5 (number)
    //   1234abcd (ctmid)  line 4, col 12, len 8, type=5 (number)
    assert_eq!(
        data,
        vec![
            0, 0, 7, 1, 0, // STATUS:
            2, 0, 2, 3, 0, // M
            2, 0, 1, 6, 0, // @
            0, 3, 8, 5, 0, // qpvuntsm
            0, 9, 8, 5, 0, // 1234abcd (delta from previous: 12 - 3 = 9)
        ]
    );
}

#[test]
fn lsp_semantic_tokens_full_unknown_buffer_uri_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let uri = format!("file://{}/other.txt", dir.path().display());
    session.did_open(&uri, "plaintext", "hello world\n");

    let resp = session.request(
        "textDocument/semanticTokens/full",
        serde_json::json!({
            "textDocument": { "uri": uri }
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "semanticTokens/full returned error: {resp}"
    );
    let data = resp["result"]["data"].as_array().expect("data array");
    assert!(data.is_empty(), "expected empty tokens, got: {data:?}");
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

#[test]
fn lsp_did_open_empty_status_jujutsu_auto_populates_disk() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_path = dir
        .path()
        .join(".jj")
        .join("badjuju")
        .join("status.jujutsu");
    let status_uri = format!("file://{}", status_path.display());

    session.did_open(&status_uri, "jujutsu", "");

    // Server sends workspace/applyEdit after writing the file; receiving it confirms the write.
    let req = session.recv_server_request("workspace/applyEdit");
    session.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "result": { "applied": true }
    }));

    let content = std::fs::read_to_string(&status_path).unwrap();
    assert!(
        content.contains("@  :"),
        "status.jujutsu missing @   header"
    );
    assert!(content.contains("STACK:"), "status.jujutsu missing STACK:");
}

#[test]
fn lsp_goto_implementation_opens_file_in_working_copy() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    std::fs::write(dir.path().join("readme.txt"), "line1\nline2\nline3\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "add readme"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    session.did_open(&status_uri, "jujutsu", &status_content);

    // Find a line in the WORKING COPY CHANGES section that names readme.txt.
    let (file_line, _) = status_content
        .lines()
        .enumerate()
        .find(|(_, l)| *l == "readme.txt")
        .map(|(i, l)| (i, l.to_string()))
        .expect("expected readme.txt in status buffer");

    let resp = session.request(
        "textDocument/implementation",
        serde_json::json!({
            "textDocument": { "uri": status_uri },
            "position": { "line": file_line, "character": 0 }
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "goto_implementation returned error: {resp}"
    );
    let target_uri = resp["result"]["uri"].as_str().unwrap_or("");
    assert!(
        target_uri.starts_with("file://"),
        "expected file:// URI: {target_uri}"
    );
    assert!(
        target_uri.ends_with("/readme.txt"),
        "expected target to be readme.txt: {target_uri}"
    );
    // Bare filename row → no hunk above → line 0.
    assert_eq!(resp["result"]["range"]["start"]["line"], 0);
}

#[test]
fn lsp_goto_implementation_missing_file_warns_and_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    // Commit a file then move on and delete it from the working copy.
    std::fs::write(dir.path().join("gone.txt"), "v1\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "add gone"])
        .current_dir(dir.path())
        .output()
        .unwrap();

    // Move forward and delete the file from the working copy. @ now has a
    // "D gone.txt" change; the file is absent on disk.
    Command::new("jj")
        .args(["new", "-m", "remove gone"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    std::fs::remove_file(dir.path().join("gone.txt")).unwrap();

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    session.did_open(&status_uri, "jujutsu", &status_content);

    let (file_line, _) = status_content
        .lines()
        .enumerate()
        .find(|(_, l)| l.trim_end() == "gone.txt")
        .map(|(i, l)| (i, l.to_string()))
        .expect("expected gone.txt line in status buffer");

    // Send the request manually so we can capture the show_message
    // notification that the handler emits *before* its response — the normal
    // `request` helper consumes and discards intermediate notifications.
    let id = session.next_id;
    session.next_id += 1;
    session.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "textDocument/implementation",
        "params": {
            "textDocument": { "uri": &status_uri },
            "position": { "line": file_line, "character": 0 }
        }
    }));
    let expected_id = serde_json::json!(id);
    let mut warn_msg: Option<serde_json::Value> = None;
    let resp = loop {
        let msg = session.recv();
        if msg.get("id") == Some(&expected_id) {
            break msg;
        }
        if msg.get("method").and_then(|v| v.as_str()) == Some("window/showMessage") {
            warn_msg = Some(msg);
        }
    };
    // tower_lsp's async pipeline can deliver the response before the
    // show_message notification awaited just prior. Drain briefly to catch
    // any straggler notification.
    if warn_msg.is_none()
        && let Some(msg) = session.try_recv(Duration::from_millis(500))
        && msg.get("method").and_then(|v| v.as_str()) == Some("window/showMessage")
    {
        warn_msg = Some(msg);
    }
    assert!(
        resp.get("error").is_none(),
        "goto_implementation returned error: {resp}"
    );
    assert!(
        resp["result"].is_null(),
        "expected null result for missing file; got: {}",
        resp["result"]
    );
    let msg = warn_msg.expect("expected window/showMessage warning for missing file");
    let typ = msg["params"]["type"].as_u64().unwrap_or(0);
    let text = msg["params"]["message"].as_str().unwrap_or("");
    // MessageType::WARNING == 2 (per the LSP spec).
    assert_eq!(typ, 2, "expected WARNING (2); got type={typ}");
    assert!(
        text.contains("gone.txt"),
        "expected message to mention gone.txt; got: {text}"
    );
}

#[test]
fn lsp_text_document_content_serves_file_blob_at_commit() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    // Pin a known file at a known commit-id.
    std::fs::write(dir.path().join("readme.txt"), "hello world\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "add readme"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let commit_id = String::from_utf8(
        Command::new("jj")
            .args([
                "log",
                "-r",
                "@",
                "--no-graph",
                "--template",
                "commit_id",
                "--limit",
                "1",
            ])
            .current_dir(dir.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    // Mutate the working copy on a child change — the resolver must still
    // see the pinned commit's content, not the current @ contents.
    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    std::fs::write(dir.path().join("readme.txt"), "different\n").unwrap();

    let file_uri = format!("badjuju-file:///commit/{commit_id}/readme.txt");
    let resp = session.request(
        "workspace/textDocumentContent",
        serde_json::json!({ "uri": file_uri }),
    );
    assert!(
        resp.get("error").is_none(),
        "textDocumentContent returned error: {resp}"
    );
    let text = resp["result"]["text"].as_str().unwrap_or("");
    assert_eq!(text, "hello world\n");
}

#[test]
fn lsp_text_document_content_diff_uri_still_works() {
    // Regression: the badjuju-diff:// scheme must keep working after the
    // multi-scheme dispatch refactor.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    std::fs::write(dir.path().join("foo.txt"), "x\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "add foo"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let commit_id = String::from_utf8(
        Command::new("jj")
            .args([
                "log",
                "-r",
                "@",
                "--no-graph",
                "--template",
                "commit_id",
                "--limit",
                "1",
            ])
            .current_dir(dir.path())
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let diff_uri = format!("badjuju-diff:///commit/{commit_id}");
    let resp = session.request(
        "workspace/textDocumentContent",
        serde_json::json!({ "uri": diff_uri }),
    );
    assert!(
        resp.get("error").is_none(),
        "diff textDocumentContent returned error: {resp}"
    );
    let text = resp["result"]["text"].as_str().unwrap_or("");
    assert!(
        text.starts_with("COMMIT_ID:"),
        "expected COMMIT_ID header in diff content; got:\n{text}"
    );
}

#[test]
fn lsp_text_document_content_unknown_scheme_errors() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let resp = session.request(
        "workspace/textDocumentContent",
        serde_json::json!({ "uri": "file:///etc/passwd" }),
    );
    assert!(
        resp.get("error").is_some(),
        "expected error for unknown URI scheme; got: {resp}"
    );
}

#[test]
fn lsp_goto_definition_on_commit_line_opens_diff() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    let (commit_line, _) = first_commit_line(&status_content)
        .unwrap_or_else(|| panic!("no commit line in status buffer:\n{status_content}"));

    session.did_open(&status_uri, "jujutsu", &status_content);

    let resp = session.request(
        "textDocument/definition",
        serde_json::json!({
            "textDocument": { "uri": status_uri },
            "position": { "line": commit_line, "character": 0 }
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "goto_definition returned error: {resp}"
    );
    let result = &resp["result"];
    assert!(
        !result.is_null(),
        "goto_definition should return a location, got null"
    );
    let target_uri = result["uri"].as_str().unwrap_or("");
    assert!(
        target_uri.ends_with(".jujutsu") && target_uri.contains("/diff-"),
        "expected diff URI (diff-change-*.jujutsu or diff-commit-*.jujutsu), got: {target_uri}"
    );
}

#[test]
fn lsp_folding_range_returns_ranges_for_commits_in_stack() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);

    session.did_open(&status_uri, "jujutsu", &status_content);

    let resp = session.request(
        "textDocument/foldingRange",
        serde_json::json!({
            "textDocument": { "uri": status_uri }
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "foldingRange returned error: {resp}"
    );
    let ranges = resp["result"].as_array().expect("expected array result");
    for range in ranges {
        assert!(
            range["startLine"].as_u64() < range["endLine"].as_u64(),
            "each folding range must span multiple lines: {range}"
        );
    }
}

#[test]
fn lsp_folding_range_returns_nested_ranges_for_changes_sections() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    // Create a file in parent, then modify it in @.
    std::fs::write(dir.path().join("foo.txt"), "old line\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "parent"])
        .current_dir(dir.path())
        .output()
        .expect("jj describe failed");
    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .expect("jj new failed");
    std::fs::write(dir.path().join("foo.txt"), "new line\n").unwrap();

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);

    assert!(
        status_content.contains("WORKING COPY CHANGES"),
        "expected WORKING COPY CHANGES in status:\n{status_content}"
    );

    session.did_open(&status_uri, "jujutsu", &status_content);

    let resp = session.request(
        "textDocument/foldingRange",
        serde_json::json!({
            "textDocument": { "uri": status_uri }
        }),
    );
    assert!(
        resp.get("error").is_none(),
        "foldingRange returned error: {resp}"
    );
    let ranges = resp["result"].as_array().expect("expected array result");

    // Find the section fold (starts at WORKING COPY CHANGES line).
    let lines: Vec<&str> = status_content.lines().collect();
    let section_line = lines
        .iter()
        .position(|l| l.starts_with("WORKING COPY CHANGES"))
        .expect("expected WORKING COPY CHANGES line") as u64;
    let file_line = lines
        .iter()
        .position(|l| *l == "foo.txt")
        .expect("expected foo.txt line") as u64;

    let section_fold = ranges
        .iter()
        .find(|r| r["startLine"].as_u64() == Some(section_line))
        .expect("expected section fold starting at WORKING COPY CHANGES line");
    let file_fold = ranges
        .iter()
        .find(|r| r["startLine"].as_u64() == Some(file_line))
        .expect("expected file fold starting at foo.txt line");

    // File fold must be contained within section fold.
    assert!(
        file_fold["startLine"].as_u64() >= section_fold["startLine"].as_u64()
            && file_fold["endLine"].as_u64() <= section_fold["endLine"].as_u64(),
        "file fold must be inside section fold: file={file_fold} section={section_fold}"
    );

    // There must be a hunk fold (starts at @@) contained within the file fold.
    let hunk_fold = ranges.iter().find(|r| {
        let sl = r["startLine"].as_u64().unwrap_or(0);
        lines.get(sl as usize).is_some_and(|l| l.starts_with("@@"))
    });
    let hunk_fold = hunk_fold.expect("expected a hunk fold starting at @@");
    assert!(
        hunk_fold["startLine"].as_u64() >= file_fold["startLine"].as_u64()
            && hunk_fold["endLine"].as_u64() <= file_fold["endLine"].as_u64(),
        "hunk fold must be inside file fold: hunk={hunk_fold} file={file_fold}"
    );
}

#[test]
fn lsp_status_merge_commit_emits_two_parent_changes_sections() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    // Build a merge commit: two parents each with a unique file.
    std::fs::write(dir.path().join("from-a.txt"), "a\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "branch-a"])
        .current_dir(dir.path())
        .output()
        .expect("jj describe failed");
    let a_id = String::from_utf8(
        Command::new("jj")
            .args([
                "log",
                "--no-pager",
                "-r",
                "@",
                "--no-graph",
                "-T",
                "change_id",
            ])
            .current_dir(dir.path())
            .output()
            .expect("jj log failed")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .expect("jj new failed");
    std::fs::write(dir.path().join("from-b.txt"), "b\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "branch-b"])
        .current_dir(dir.path())
        .output()
        .expect("jj describe failed");
    let b_id = String::from_utf8(
        Command::new("jj")
            .args([
                "log",
                "--no-pager",
                "-r",
                "@",
                "--no-graph",
                "-T",
                "change_id",
            ])
            .current_dir(dir.path())
            .output()
            .expect("jj log failed")
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    Command::new("jj")
        .args(["new", &a_id, &b_id])
        .current_dir(dir.path())
        .output()
        .expect("jj new (merge) failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);

    let parent_sections: Vec<_> = status_content
        .lines()
        .filter(|l| l.starts_with("PARENT CHANGES ("))
        .collect();
    assert_eq!(
        parent_sections.len(),
        2,
        "expected two PARENT CHANGES sections for merge commit:\n{status_content}"
    );
    assert!(
        status_content.contains("from-a.txt"),
        "expected from-a.txt:\n{status_content}"
    );
    assert!(
        status_content.contains("from-b.txt"),
        "expected from-b.txt:\n{status_content}"
    );
}

/// When a client that does NOT advertise virtualDiffs opens status.jujutsu and
/// an external `jj` operation fires the op-head watcher, the server should
/// push the regenerated content to the client via `workspace/applyEdit`. This
/// is the Helix fallback path (no auto-reload, no virtual content support).
#[test]
fn lsp_apply_edit_sent_to_file_based_client_on_external_op() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    session.did_open(&status_uri, "jujutsu", &status_content);

    // Trigger an external op-head change so the watcher refreshes status.
    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .expect("jj new failed");

    let req = session
        .recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(3000))
        .expect("expected workspace/applyEdit within 3s after external jj new");

    let changes = req["params"]["edit"]["changes"]
        .as_object()
        .expect("edit.changes should be an object");
    let edits = changes
        .get(&status_uri)
        .unwrap_or_else(|| panic!("no edits for {status_uri} in: {req}"))
        .as_array()
        .expect("edits should be an array");
    let new_text = edits[0]["newText"]
        .as_str()
        .expect("newText should be a string");

    // Acknowledge the request so the server's apply_edit() call returns.
    session.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "result": { "applied": true }
    }));

    // The pushed content must match what's on disk — the server should have
    // sent the same string it wrote, not re-read it.
    let on_disk = std::fs::read_to_string(status_uri.strip_prefix("file://").unwrap()).unwrap();
    assert_eq!(
        new_text, on_disk,
        "applyEdit newText should match regenerated file content"
    );
    assert!(
        new_text.contains("@  :"),
        "applyEdit content should be a fresh status buffer:\n{new_text}"
    );
}

/// When the client advertises `virtualDiffs: true` (VS Code, Neovim), the
/// server must NOT send `workspace/applyEdit` on external ops — those clients
/// handle refresh via FileSystemWatcher (VS Code) or autoreload (Neovim).
/// Guards against regressing those clients while adding the Helix fallback.
#[test]
fn lsp_apply_edit_suppressed_when_virtual_diffs_enabled() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize_with_options(&root_uri, serde_json::json!({ "virtualDiffs": true }));

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    session.did_open(&status_uri, "jujutsu", &status_content);

    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .expect("jj new failed");

    // Wait well past the 500ms debounce; no applyEdit should arrive.
    let msg =
        session.recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(2000));
    assert!(
        msg.is_none(),
        "expected no applyEdit when virtualDiffs is enabled, got: {msg:?}"
    );
}

#[test]
fn lsp_squash_commit_does_not_rewrite_status_buffer() {
    // Source selection must signal pending state out-of-band (diagnostics +
    // showMessage), not by rewriting status.jujutsu. This keeps user-opened
    // folds intact across `s`.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let before = read_file(&status_uri);

    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "squash.commit returned error: {resp}"
    );

    let after = read_file(&status_uri);
    assert_eq!(
        before, after,
        "status.jujutsu must not be rewritten on source selection"
    );
    assert!(
        !after.contains("Preparing to squash"),
        "no inline marker should be injected into status:\n{after}"
    );
}

#[test]
fn lsp_squash_cancel_does_not_rewrite_status_buffer() {
    // Cancel clears server-side pending state via diagnostics + showMessage;
    // status.jujutsu is not regenerated.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let before = read_file(&status_uri);

    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    assert!(resp.get("error").is_none(), "squash.commit failed: {resp}");

    let resp = session.execute_command_with_args("badjuju.squash.cancel", serde_json::json!([]));
    assert!(resp.get("error").is_none(), "squash.cancel failed: {resp}");

    let after = read_file(&status_uri);
    assert_eq!(
        before, after,
        "status.jujutsu must not be rewritten on cancel"
    );
    assert!(
        !after.contains("Preparing to squash"),
        "no inline marker should appear in status:\n{after}"
    );
}

#[test]
fn lsp_squash_commit_destination_selection_opens_squash_window() {
    // After squash.commit selects a source, a second squash.commit call selects
    // the destination and must return a squash window URI (SQUASHING: header).
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    // Two commits so source and destination are distinct.
    Command::new("jj")
        .args(["describe", "-m", "source commit"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "destination commit"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    // First call: source selection (@-).
    let resp =
        session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@-"]));
    assert!(
        resp.get("error").is_none(),
        "source selection failed: {resp}"
    );

    // Second call: destination selection (@).
    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "destination selection failed: {resp}"
    );
    let squash_uri = resp["result"].as_str().unwrap();
    assert!(
        squash_uri.contains("/squash/") && squash_uri.ends_with(".jujutsu"),
        "expected squash window URI, got: {squash_uri}"
    );
    let squash_content = read_file(squash_uri);
    assert!(
        squash_content.starts_with("SQUASHING:"),
        "expected SQUASHING: header:\n{squash_content}"
    );
    assert!(
        squash_content.contains("From:"),
        "expected From: row:\n{squash_content}"
    );
    assert!(
        squash_content.contains("To:"),
        "expected To: row:\n{squash_content}"
    );
    assert!(
        squash_content.contains("SELECTED CHANGES:"),
        "expected SELECTED CHANGES section:\n{squash_content}"
    );
    assert!(
        squash_content.contains("REMAINING CHANGES:"),
        "expected REMAINING CHANGES section:\n{squash_content}"
    );
}

#[test]
fn lsp_code_action_with_pending_squash_shows_cancel() {
    // When squash.commit is pending, code actions on a commit row must show
    // "Cancel pending squash" (badjuju.squash.cancel) instead of "Squash from
    // this revision" (badjuju.squash.commit).
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "second"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    session.did_open(&status_uri, "jujutsu", &status_content);

    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    assert!(resp.get("error").is_none(), "squash.commit failed: {resp}");

    let (commit_line, _) = first_commit_line(&status_content)
        .unwrap_or_else(|| panic!("no commit line in status:\n{status_content}"));
    let actions = code_action_at(&mut session, &status_uri, commit_line);

    let into_action = actions
        .iter()
        .find(|a| a["title"].as_str() == Some("Squash into this revision"))
        .expect("expected 'Squash into this revision' action when pending");
    assert_eq!(
        into_action["command"]["command"].as_str(),
        Some("badjuju.squash.commit"),
        "Squash into this revision should invoke badjuju.squash.commit"
    );

    let cancel_action = actions
        .iter()
        .find(|a| a["title"].as_str() == Some("Cancel pending squash"))
        .expect("expected 'Cancel pending squash' action when pending");
    assert_eq!(
        cancel_action["command"]["command"].as_str(),
        Some("badjuju.squash.cancel"),
        "Cancel pending squash should invoke badjuju.squash.cancel"
    );
}

#[test]
fn lsp_code_action_on_status_non_at_commit_with_pending_squash_shows_into() {
    // Regression for #23: in the status buffer's stack section, code actions
    // on a non-@ commit row (e.g. @-) must still surface "Squash into this
    // revision" when a squash source is pending.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "second"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let status_content = read_file(&status_uri);
    session.did_open(&status_uri, "jujutsu", &status_content);

    // Pending source: @.
    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    assert!(resp.get("error").is_none(), "squash.commit failed: {resp}");

    // Find the @- commit row (second commit-header line).
    let commit_lines: Vec<usize> = status_content
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let first = l.chars().next()?;
            matches!(first, '@' | '○' | '●' | '◆' | '*').then_some(i)
        })
        .collect();
    assert!(
        commit_lines.len() >= 2,
        "test setup needs at least two commit rows in status:\n{status_content}"
    );
    let dest_line = commit_lines[1];

    let actions = code_action_at(&mut session, &status_uri, dest_line);
    let titles: Vec<&str> = actions.iter().filter_map(|a| a["title"].as_str()).collect();
    assert!(
        titles.iter().any(|t| *t == "Squash into this revision"),
        "expected 'Squash into this revision' on @- row in status when squash is pending, got: {titles:?}"
    );
}

#[test]
fn lsp_code_action_on_different_commit_with_pending_squash_shows_into() {
    // Regression for #23: after `s` on commit A sets the pending source,
    // requesting code actions on a different commit row (commit B) must
    // include "Squash into this revision". The previous behavior only
    // surfaced the "into" action on the same row as the source.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    Command::new("jj")
        .args(["describe", "-m", "first"])
        .current_dir(dir.path())
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "second"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");
    Command::new("jj")
        .args(["new", "-m", "third"])
        .current_dir(dir.path())
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let log_uri = session.execute_command("badjuju.log");
    let log_content = read_file(&log_uri);
    session.did_open(&log_uri, "jujutsu", &log_content);

    // Collect all commit-header line indices to find a second commit row.
    let commit_lines: Vec<usize> = log_content
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let first = l.chars().next()?;
            matches!(first, '@' | '○' | '●' | '◆' | '*').then_some(i)
        })
        .collect();
    assert!(
        commit_lines.len() >= 2,
        "test setup needs at least two commits in the log, got: {log_content}"
    );
    let source_line = commit_lines[0];
    let dest_line = commit_lines[1];

    // Select the source via cursor-form arg pointing at the first commit row.
    let resp = session.execute_command_with_args(
        "badjuju.squash.commit",
        serde_json::json!([{
            "cursor": { "uri": &log_uri, "line": source_line }
        }]),
    );
    assert!(resp.get("error").is_none(), "squash.commit failed: {resp}");

    // Ask for code actions on the *different* commit row (the destination).
    let actions = code_action_at(&mut session, &log_uri, dest_line);
    let titles: Vec<&str> = actions.iter().filter_map(|a| a["title"].as_str()).collect();
    assert!(
        titles.iter().any(|t| *t == "Squash into this revision"),
        "expected 'Squash into this revision' on different commit row when squash is pending, got: {titles:?}"
    );
}

/// Open a squash window between two commits and return the URI + content.
/// Helper shared by squash window tests.
fn open_squash_window(dir: &std::path::Path) -> (std::string::String, std::string::String) {
    // Two commits: @- has a file change, @ is the destination.
    std::fs::write(dir.join("readme.txt"), "v1\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "source commit"])
        .current_dir(dir)
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "destination commit"])
        .current_dir(dir)
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.display());
    let mut session = LspSession::start(dir);
    session.initialize(&root_uri);

    // Source = @- (has changes), Destination = @.
    let resp =
        session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@-"]));
    assert!(
        resp.get("error").is_none(),
        "source selection failed: {resp}"
    );

    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "destination selection failed: {resp}"
    );

    let squash_uri = resp["result"].as_str().unwrap().to_string();
    let squash_content = read_file(&squash_uri);
    (squash_uri, squash_content)
}

#[test]
fn lsp_squash_window_remaining_contains_source_hunks() {
    // REMAINING CHANGES must contain the file changed in the source commit.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (_squash_uri, squash_content) = open_squash_window(dir.path());

    assert!(
        squash_content.contains("readme.txt"),
        "expected readme.txt in REMAINING CHANGES:\n{squash_content}"
    );
    // SELECTED CHANGES must be empty at open time.
    let selected_start = squash_content
        .find("SELECTED CHANGES:")
        .expect("missing SELECTED CHANGES");
    let remaining_start = squash_content
        .find("REMAINING CHANGES:")
        .expect("missing REMAINING CHANGES");
    let between = &squash_content[selected_start + "SELECTED CHANGES:".len()..remaining_start];
    assert!(
        between.trim().is_empty(),
        "SELECTED CHANGES must be empty at open time; got:\n{between}"
    );
}

#[test]
fn lsp_squash_window_folding_ranges_cover_sections() {
    // squash_folding_ranges must emit at least one range per non-empty section.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (squash_uri, squash_content) = open_squash_window(dir.path());

    let mut session = LspSession::start(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    session.initialize(&root_uri);
    session.did_open(&squash_uri, "jujutsu", &squash_content);

    let resp = session.request(
        "textDocument/foldingRange",
        serde_json::json!({ "textDocument": { "uri": squash_uri } }),
    );
    assert!(resp.get("error").is_none(), "foldingRange error: {resp}");
    let ranges = resp["result"].as_array().expect("array result");

    // There must be at least file and hunk folds for the non-empty REMAINING section.
    assert!(
        !ranges.is_empty(),
        "expected folding ranges for squash window:\n{squash_content}"
    );
    for range in ranges {
        assert!(
            range["startLine"].as_u64() < range["endLine"].as_u64(),
            "each fold must span at least 2 lines: {range}"
        );
    }
}

#[test]
fn lsp_squash_window_did_close_deletes_file() {
    // Closing the squash buffer must delete the on-disk file and clear open_squash_window.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (squash_uri, squash_content) = open_squash_window(dir.path());

    // Verify file exists on disk before close.
    let squash_path = squash_uri.strip_prefix("file://").unwrap();
    assert!(
        std::path::Path::new(squash_path).exists(),
        "expected squash file on disk before close"
    );

    // Start a fresh session and open then close the squash buffer.
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);
    session.did_open(&squash_uri, "jujutsu", &squash_content);
    session.notify(
        "textDocument/didClose",
        serde_json::json!({ "textDocument": { "uri": squash_uri } }),
    );

    // Give the server a moment to process the notification.
    std::thread::sleep(Duration::from_millis(200));

    assert!(
        !std::path::Path::new(squash_path).exists(),
        "expected squash file deleted after didClose"
    );
}

/// Open a squash window in a fresh LSP session and return the session (with
/// `open_squash_window` set), the squash buffer URI, and its initial content.
/// The repo has one source commit (adds readme.txt v1) and one destination commit.
/// Like [`setup_squash_session`], but initializes the session with
/// `virtualDiffs: true` (matching VS Code / Neovim capability advertisement).
fn setup_squash_session_virtual_diffs(dir: &std::path::Path) -> (LspSession, String, String) {
    std::fs::write(dir.join("readme.txt"), "v1\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "source commit"])
        .current_dir(dir)
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "destination commit"])
        .current_dir(dir)
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.display());
    let mut session = LspSession::start(dir);
    session.initialize_with_options(&root_uri, serde_json::json!({ "virtualDiffs": true }));

    let resp =
        session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@-"]));
    assert!(
        resp.get("error").is_none(),
        "source selection failed: {resp}"
    );

    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "destination selection failed: {resp}"
    );

    let squash_uri = resp["result"].as_str().unwrap().to_string();
    let squash_content = read_file(&squash_uri);
    (session, squash_uri, squash_content)
}

#[test]
fn lsp_squash_toggle_skips_disk_write_when_virtual_diffs_enabled() {
    // Regression for #24: when the client advertises virtualDiffs (VS Code,
    // Neovim), squash buffer regenerations must NOT rewrite the on-disk file —
    // the applyEdit alone delivers the new content. Disk rewrites trigger
    // Neovim's autoreload, which re-runs the ftplugin and resets user-opened
    // folds.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (mut session, squash_uri, initial_content) = setup_squash_session_virtual_diffs(dir.path());

    session.did_open(&squash_uri, "jujutsu", &initial_content);

    let remaining_line = initial_content
        .lines()
        .enumerate()
        .find_map(|(i, l)| (l == "REMAINING CHANGES:").then_some(i))
        .expect("REMAINING CHANGES: not found in initial squash buffer");
    let hunk_line = initial_content
        .lines()
        .enumerate()
        .skip(remaining_line)
        .find_map(|(i, l)| l.starts_with("@@").then_some(i))
        .expect("no @@ hunk found in REMAINING section");

    let resp = session.execute_command_acked(
        "badjuju.squash.toggle",
        serde_json::json!([{ "cursor": { "uri": squash_uri, "line": hunk_line } }]),
    );
    assert!(resp.get("error").is_none(), "squash.toggle failed: {resp}");

    // Disk content must be unchanged from the initial creation — the toggle
    // only sent an applyEdit.
    let on_disk_after = read_file(&squash_uri);
    assert_eq!(
        on_disk_after, initial_content,
        "disk file should NOT have been rewritten when virtualDiffs is enabled; \
         applyEdit alone updates the buffer"
    );
}

fn setup_squash_session(dir: &std::path::Path) -> (LspSession, String, String) {
    std::fs::write(dir.join("readme.txt"), "v1\n").unwrap();
    Command::new("jj")
        .args(["describe", "-m", "source commit"])
        .current_dir(dir)
        .output()
        .expect("describe failed");
    Command::new("jj")
        .args(["new", "-m", "destination commit"])
        .current_dir(dir)
        .output()
        .expect("new failed");

    let root_uri = format!("file://{}", dir.display());
    let mut session = LspSession::start(dir);
    session.initialize(&root_uri);

    let resp =
        session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@-"]));
    assert!(
        resp.get("error").is_none(),
        "source selection failed: {resp}"
    );

    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "destination selection failed: {resp}"
    );

    let squash_uri = resp["result"].as_str().unwrap().to_string();
    let squash_content = read_file(&squash_uri);
    (session, squash_uri, squash_content)
}

#[test]
fn lsp_squash_toggle_hunk_moves_remaining_to_selected() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (mut session, squash_uri, squash_content) = setup_squash_session(dir.path());

    session.did_open(&squash_uri, "jujutsu", &squash_content);

    // Find the first @@ line that appears after the REMAINING CHANGES: header.
    let remaining_line = squash_content
        .lines()
        .enumerate()
        .find_map(|(i, l)| (l == "REMAINING CHANGES:").then_some(i))
        .expect("REMAINING CHANGES: not found in initial squash buffer");
    let hunk_line = squash_content
        .lines()
        .enumerate()
        .skip(remaining_line)
        .find_map(|(i, l)| l.starts_with("@@").then_some(i))
        .expect("no @@ hunk found in REMAINING section");

    let resp = session.execute_command_acked(
        "badjuju.squash.toggle",
        serde_json::json!([{ "cursor": { "uri": squash_uri, "line": hunk_line } }]),
    );
    assert!(resp.get("error").is_none(), "squash.toggle failed: {resp}");
    let new_uri = resp["result"].as_str().unwrap();
    let new_content = read_file(new_uri);

    let selected_start = new_content
        .find("SELECTED CHANGES:")
        .expect("SELECTED CHANGES: missing after toggle");
    let remaining_start = new_content
        .find("REMAINING CHANGES:")
        .expect("REMAINING CHANGES: missing after toggle");
    let selected_section = &new_content[selected_start..remaining_start];
    let remaining_section = {
        let after = &new_content[remaining_start..];
        let cmd = after.find("COMMAND REFERENCE:").unwrap_or(after.len());
        &after[..cmd]
    };

    assert!(
        selected_section.contains("@@"),
        "hunk should be in SELECTED after toggle:\n{new_content}"
    );
    assert!(
        !remaining_section.contains("@@"),
        "REMAINING should have no hunks after toggle:\n{new_content}"
    );
}

#[test]
fn lsp_squash_toggle_hunk_moves_selected_to_remaining() {
    // Move all hunks to SELECTED first via select_all, then toggle one back.
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (mut session, squash_uri, squash_content) = setup_squash_session(dir.path());

    session.did_open(&squash_uri, "jujutsu", &squash_content);

    // select_all moves everything from REMAINING to SELECTED.
    let resp = session.execute_command_acked("badjuju.squash.select_all", serde_json::json!([]));
    assert!(resp.get("error").is_none(), "select_all failed: {resp}");
    let after_all_uri = resp["result"].as_str().unwrap().to_string();
    let after_all_content = read_file(&after_all_uri);

    // Find first @@ line in SELECTED section of the post-select_all content.
    let selected_line = after_all_content
        .lines()
        .enumerate()
        .find_map(|(i, l)| (l == "SELECTED CHANGES:").then_some(i))
        .expect("SELECTED CHANGES: not found after select_all");
    let remaining_line = after_all_content
        .lines()
        .enumerate()
        .skip(selected_line)
        .find_map(|(i, l)| (l == "REMAINING CHANGES:").then_some(i))
        .expect("REMAINING CHANGES: not found after select_all");
    let hunk_line = after_all_content
        .lines()
        .enumerate()
        .skip(selected_line)
        .take(remaining_line - selected_line)
        .find_map(|(i, l)| l.starts_with("@@").then_some(i))
        .expect("no @@ hunk in SELECTED section after select_all");

    let resp = session.execute_command_acked(
        "badjuju.squash.toggle",
        serde_json::json!([{ "cursor": { "uri": squash_uri, "line": hunk_line } }]),
    );
    assert!(
        resp.get("error").is_none(),
        "squash.toggle (selected→remaining) failed: {resp}"
    );
    let new_uri = resp["result"].as_str().unwrap();
    let new_content = read_file(new_uri);

    let sel_start = new_content
        .find("SELECTED CHANGES:")
        .expect("SELECTED CHANGES: missing");
    let rem_start = new_content
        .find("REMAINING CHANGES:")
        .expect("REMAINING CHANGES: missing");
    let selected_section = &new_content[sel_start..rem_start];
    let remaining_section = &new_content[rem_start..];

    assert!(
        !selected_section.contains("@@"),
        "SELECTED should be empty after toggle back:\n{new_content}"
    );
    assert!(
        remaining_section.contains("@@"),
        "hunk should be back in REMAINING:\n{new_content}"
    );
}

#[test]
fn lsp_squash_toggle_file_moves_remaining_to_selected() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (mut session, squash_uri, squash_content) = setup_squash_session(dir.path());

    session.did_open(&squash_uri, "jujutsu", &squash_content);

    // Find the plain file-path line (readme.txt) in REMAINING — not @@, not a diff line.
    let remaining_line = squash_content
        .lines()
        .enumerate()
        .find_map(|(i, l)| (l == "REMAINING CHANGES:").then_some(i))
        .expect("REMAINING CHANGES: not found");
    let file_line = squash_content
        .lines()
        .enumerate()
        .skip(remaining_line + 1)
        .find_map(|(i, l)| {
            let plain = !l.is_empty()
                && !l.starts_with("@@")
                && !l.starts_with('+')
                && !l.starts_with('-')
                && !l.starts_with(' ')
                && !l.starts_with("COMMAND");
            plain.then_some(i)
        })
        .expect("no file-path line found in REMAINING section");

    let resp = session.execute_command_acked(
        "badjuju.squash.toggle",
        serde_json::json!([{ "cursor": { "uri": squash_uri, "line": file_line } }]),
    );
    assert!(
        resp.get("error").is_none(),
        "squash.toggle (file) failed: {resp}"
    );
    let new_uri = resp["result"].as_str().unwrap();
    let new_content = read_file(new_uri);

    let sel_start = new_content
        .find("SELECTED CHANGES:")
        .expect("SELECTED CHANGES: missing");
    let rem_start = new_content
        .find("REMAINING CHANGES:")
        .expect("REMAINING CHANGES: missing");
    let selected_section = &new_content[sel_start..rem_start];

    assert!(
        selected_section.contains("readme.txt"),
        "file should be in SELECTED after file-level toggle:\n{new_content}"
    );
    assert!(
        !selected_section.is_empty(),
        "SELECTED section should be non-trivial:\n{new_content}"
    );
}

#[test]
fn lsp_squash_select_all_empties_remaining() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (mut session, squash_uri, squash_content) = setup_squash_session(dir.path());

    session.did_open(&squash_uri, "jujutsu", &squash_content);

    let resp = session.execute_command_acked("badjuju.squash.select_all", serde_json::json!([]));
    assert!(resp.get("error").is_none(), "select_all failed: {resp}");
    let new_uri = resp["result"].as_str().unwrap();
    let new_content = read_file(new_uri);

    let sel_start = new_content
        .find("SELECTED CHANGES:")
        .expect("SELECTED CHANGES: missing");
    let rem_start = new_content
        .find("REMAINING CHANGES:")
        .expect("REMAINING CHANGES: missing");
    let selected_section = &new_content[sel_start..rem_start];
    let remaining_after = &new_content[rem_start + "REMAINING CHANGES:".len()..];
    let remaining_body = remaining_after
        .find("COMMAND REFERENCE:")
        .map(|i| &remaining_after[..i])
        .unwrap_or(remaining_after);

    assert!(
        selected_section.contains("@@"),
        "SELECTED should have hunks after select_all:\n{new_content}"
    );
    assert!(
        !remaining_body.contains("@@"),
        "REMAINING should have no hunks after select_all:\n{new_content}"
    );
}

#[test]
fn lsp_squash_select_none_repopulates_remaining() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (mut session, squash_uri, squash_content) = setup_squash_session(dir.path());

    session.did_open(&squash_uri, "jujutsu", &squash_content);

    // Move everything to SELECTED first.
    let resp = session.execute_command_acked("badjuju.squash.select_all", serde_json::json!([]));
    assert!(resp.get("error").is_none(), "select_all failed: {resp}");

    // Then move it all back to REMAINING.
    let resp = session.execute_command_acked("badjuju.squash.select_none", serde_json::json!([]));
    assert!(resp.get("error").is_none(), "select_none failed: {resp}");
    let new_uri = resp["result"].as_str().unwrap();
    let new_content = read_file(new_uri);

    let sel_start = new_content
        .find("SELECTED CHANGES:")
        .expect("SELECTED CHANGES: missing");
    let rem_start = new_content
        .find("REMAINING CHANGES:")
        .expect("REMAINING CHANGES: missing");
    let selected_section = &new_content[sel_start..rem_start];
    let remaining_section = &new_content[rem_start..];

    assert!(
        !selected_section.contains("@@"),
        "SELECTED should be empty after select_none:\n{new_content}"
    );
    assert!(
        remaining_section.contains("@@"),
        "REMAINING should have hunks after select_none:\n{new_content}"
    );
}

#[test]
fn lsp_squash_window_regenerates_on_external_op() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (mut session, squash_uri, squash_content) = setup_squash_session(dir.path());

    // Register the squash buffer in the server's document cache.
    session.did_open(&squash_uri, "jujutsu", &squash_content);

    // Trigger an external jj operation — describe the destination commit.
    Command::new("jj")
        .args(["describe", "-r", "@", "-m", "updated destination"])
        .current_dir(dir.path())
        .output()
        .expect("jj describe failed");

    // The op-head watcher must regenerate and push the squash window via applyEdit.
    let req = session
        .recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(3000))
        .expect("expected workspace/applyEdit within 3s after external jj describe");

    let changes = req["params"]["edit"]["changes"]
        .as_object()
        .expect("edit.changes should be an object");
    let edits = changes
        .get(&squash_uri)
        .unwrap_or_else(|| panic!("no edits for {squash_uri} in: {req}"))
        .as_array()
        .expect("edits should be an array");
    let new_text = edits[0]["newText"]
        .as_str()
        .expect("newText should be a string");

    session.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "result": { "applied": true }
    }));

    assert!(
        new_text.starts_with("SQUASHING:"),
        "regenerated content should start with SQUASHING::\n{new_text}"
    );
    assert!(
        new_text.contains("updated destination"),
        "regenerated content should reflect new commit description:\n{new_text}"
    );
}

#[test]
fn lsp_squash_window_shows_notice_when_from_abandoned() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let (mut session, squash_uri, squash_content) = setup_squash_session(dir.path());

    session.did_open(&squash_uri, "jujutsu", &squash_content);

    // Abandon the source commit externally.
    Command::new("jj")
        .args(["abandon", "@-"])
        .current_dir(dir.path())
        .output()
        .expect("jj abandon failed");

    // The watcher must push a "target no longer exists" notice via applyEdit.
    let req = session
        .recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(3000))
        .expect("expected workspace/applyEdit within 3s after abandoning source");

    let changes = req["params"]["edit"]["changes"]
        .as_object()
        .expect("edit.changes should be an object");
    let edits = changes
        .get(&squash_uri)
        .unwrap_or_else(|| panic!("no edits for {squash_uri} in: {req}"))
        .as_array()
        .expect("edits should be an array");
    let new_text = edits[0]["newText"]
        .as_str()
        .expect("newText should be a string");

    session.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "result": { "applied": true }
    }));

    assert!(
        new_text.contains("SQUASH TARGET NO LONGER EXISTS"),
        "expected notice when source commit is abandoned:\n{new_text}"
    );
}

/// Open a squash window, place the cursor on the `@@` hunk line, invoke
/// `badjuju.squash.edit_hunk`, and verify the returned URI points at
/// `hunk-edit.jujutsu` with the expected JJ:-prefixed metadata + hunk body.
#[test]
fn lsp_squash_edit_hunk_opens_buffer_with_metadata() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    // Prime the working copy via the helper, then drop its session so the
    // commits remain but the window state is fresh in the new session below.
    let _ = open_squash_window(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);
    // The squash window state lives inside this session, so we need to redo
    // the commit-to-commit selection here too.
    let resp =
        session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@-"]));
    assert!(
        resp.get("error").is_none(),
        "source selection failed: {resp}"
    );
    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    assert!(
        resp.get("error").is_none(),
        "destination selection failed: {resp}"
    );
    let squash_uri = resp["result"].as_str().unwrap().to_string();
    let squash_content = read_file(&squash_uri);
    let hunk_line = squash_content
        .lines()
        .position(|l| l.starts_with("@@"))
        .expect("expected @@ line in regenerated squash window");

    session.did_open(&squash_uri, "jujutsu", &squash_content);

    let resp = session.execute_command_acked(
        "badjuju.squash.edit_hunk",
        serde_json::json!([{ "cursor": { "uri": squash_uri, "line": hunk_line } }]),
    );
    assert!(
        resp.get("error").is_none(),
        "edit_hunk returned error: {resp}"
    );
    let hunk_edit_uri = resp["result"].as_str().expect("string result");
    assert!(
        hunk_edit_uri.ends_with("/hunk-edit.jujutsu"),
        "unexpected URI: {hunk_edit_uri}"
    );
    let content = read_file(hunk_edit_uri);
    assert!(
        content.contains("JJ: action: squash"),
        "missing JJ: action:\n{content}"
    );
    assert!(
        content.contains("JJ: file: readme.txt"),
        "missing file metadata:\n{content}"
    );
    assert!(content.contains("@@"), "missing @@ header:\n{content}");
    assert!(
        content.contains("COMMAND REFERENCE:"),
        "missing reference block:\n{content}"
    );
}

/// Saving a hunk-edit buffer whose body has been cleared must abort cleanly:
/// no jj invocation, terminal notice rendered, server state cleared.
#[test]
fn lsp_hunk_edit_save_empty_body_aborts_with_notice() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let _ = open_squash_window(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);
    let resp =
        session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@-"]));
    assert!(
        resp.get("error").is_none(),
        "source selection failed: {resp}"
    );
    let resp = session.execute_command_with_args("badjuju.squash.commit", serde_json::json!(["@"]));
    let squash_uri = resp["result"].as_str().unwrap().to_string();
    let squash_content = read_file(&squash_uri);
    let hunk_line = squash_content
        .lines()
        .position(|l| l.starts_with("@@"))
        .expect("expected @@ line");

    session.did_open(&squash_uri, "jujutsu", &squash_content);

    let resp = session.execute_command_acked(
        "badjuju.squash.edit_hunk",
        serde_json::json!([{ "cursor": { "uri": squash_uri, "line": hunk_line } }]),
    );
    let hunk_edit_uri = resp["result"].as_str().unwrap().to_string();

    // Save with a body that has only JJ: lines — no hunk content.
    let empty = "JJ: nothing\n";
    session.notify(
        "textDocument/didOpen",
        serde_json::json!({
            "textDocument": {
                "uri": hunk_edit_uri,
                "languageId": "jujutsu",
                "version": 1,
                "text": empty,
            }
        }),
    );
    session.notify(
        "textDocument/didSave",
        serde_json::json!({
            "textDocument": { "uri": hunk_edit_uri },
            "text": empty,
        }),
    );

    // Expect an applyEdit pushing the EDIT ABORTED notice onto the hunk-edit buffer.
    let req = session
        .recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(3000))
        .expect("expected applyEdit with abort notice");
    ack_apply_edit(&mut session, &req);
    let new_text = req["params"]["edit"]["changes"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()[0]["newText"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        new_text.contains("EDIT ABORTED"),
        "expected abort notice, got: {new_text}"
    );
}

/// `apply_edit_if_open` skips URIs not in `state.documents`. When status was
/// generated via executeCommand but never opened, no applyEdit should fire on
/// an external op — there's no client buffer to update.
#[test]
fn lsp_apply_edit_skipped_when_document_not_open() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());

    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    // Generate status.jujutsu on disk but never did_open it.
    let _status_uri = session.execute_command("badjuju.status");

    Command::new("jj")
        .args(["new"])
        .current_dir(dir.path())
        .output()
        .expect("jj new failed");

    let msg =
        session.recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(2000));
    assert!(
        msg.is_none(),
        "expected no applyEdit when no document is open, got: {msg:?}"
    );
}

// ---- did_open cold-open refresh (#18) ----

fn ack_apply_edit(session: &mut LspSession, req: &serde_json::Value) {
    session.send(serde_json::json!({
        "jsonrpc": "2.0",
        "id": req["id"],
        "result": { "applied": true }
    }));
}

/// Helix cold-open: the user opens a `.jj/badjuju/status.jujutsu` whose
/// on-disk content is stale (left over from a previous session or external
/// `jj` op). Since the server never received a `badjuju.status` command, there
/// is no preopen mark — `did_open` must regenerate and push fresh content.
#[test]
fn lsp_did_open_refreshes_stale_status() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_path = dir
        .path()
        .join(".jj")
        .join("badjuju")
        .join("status.jujutsu");
    std::fs::create_dir_all(status_path.parent().unwrap()).unwrap();
    std::fs::write(&status_path, "STALE CONTENT\n").unwrap();
    let status_uri = format!("file://{}", status_path.display());

    session.did_open(&status_uri, "jujutsu", "STALE CONTENT\n");

    let req = session
        .recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(3000))
        .expect("expected applyEdit on stale cold-open");
    ack_apply_edit(&mut session, &req);

    let new_text = req["params"]["edit"]["changes"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()[0]["newText"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        new_text.contains("STACK:") && !new_text.contains("STALE CONTENT"),
        "applyEdit should carry fresh status content, got:\n{new_text}"
    );
}

#[test]
fn lsp_did_open_refreshes_stale_log() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let log_path = dir.path().join(".jj").join("badjuju").join("log.jujutsu");
    std::fs::create_dir_all(log_path.parent().unwrap()).unwrap();
    std::fs::write(&log_path, "STALE LOG\n").unwrap();
    let log_uri = format!("file://{}", log_path.display());

    session.did_open(&log_uri, "jujutsu", "STALE LOG\n");

    let req = session
        .recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(3000))
        .expect("expected applyEdit on stale log cold-open");
    ack_apply_edit(&mut session, &req);

    let new_text = req["params"]["edit"]["changes"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()[0]["newText"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        new_text.contains("REVSET:") && !new_text.contains("STALE LOG"),
        "applyEdit should carry fresh log content, got:\n{new_text}"
    );
}

/// Cold-opening `diff-change-<id>.jujutsu` must regenerate the diff using the
/// id parsed from the filename — not hardcoded `@`, which the previous code
/// path did. Catches that regression.
#[test]
fn lsp_did_open_refreshes_diff_using_change_id_from_uri() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    // Generate a diff buffer for @ in file mode.
    let diff_uri =
        session.execute_command_with_args("badjuju.diff", serde_json::json!(["@"]))["result"]
            .as_str()
            .unwrap()
            .to_string();
    let path = diff_uri.strip_prefix("file://").unwrap();
    // Tear down preopen mark by closing without opening, then corrupt the file.
    session.notify(
        "textDocument/didClose",
        serde_json::json!({ "textDocument": { "uri": diff_uri } }),
    );
    std::fs::write(path, "STALE DIFF\n").unwrap();

    session.did_open(&diff_uri, "jujutsu", "STALE DIFF\n");

    let req = session
        .recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(3000))
        .expect("expected applyEdit on stale diff cold-open");
    ack_apply_edit(&mut session, &req);

    let new_text = req["params"]["edit"]["changes"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap()[0]["newText"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        new_text.contains("CHANGE_ID:") && !new_text.contains("STALE DIFF"),
        "applyEdit should carry fresh diff content (with CHANGE_ID header), got:\n{new_text}"
    );
}

/// VS Code / Neovim flow: client invokes `badjuju.status`, server pre-writes
/// the file, then client opens it. The preopen mark must suppress the
/// cold-open regen so no spurious applyEdit fires.
#[test]
fn lsp_did_open_skips_regen_when_preopen_marked() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let content = read_file(&status_uri);
    session.did_open(&status_uri, "jujutsu", &content);

    let msg =
        session.recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(1500));
    assert!(
        msg.is_none(),
        "preopen-marked URI should not trigger applyEdit, got: {msg:?}"
    );
}

/// `did_close` must clear any stale preopen mark — otherwise a subsequent cold
/// open of the same URI would be wrongly suppressed.
#[test]
fn lsp_did_close_clears_preopen_mark() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let status_uri = session.execute_command("badjuju.status");
    let path = status_uri.strip_prefix("file://").unwrap();

    // Close without ever opening — drops the preopen mark.
    session.notify(
        "textDocument/didClose",
        serde_json::json!({ "textDocument": { "uri": &status_uri } }),
    );

    // Corrupt the on-disk file and reopen: refresh must fire.
    std::fs::write(path, "STALE AFTER CLOSE\n").unwrap();
    session.did_open(&status_uri, "jujutsu", "STALE AFTER CLOSE\n");

    let req = session
        .recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(3000))
        .expect("expected applyEdit after did_close cleared the preopen mark");
    ack_apply_edit(&mut session, &req);
}

/// `diff-commit-<id>.jujutsu` is pinned by design — cold-open must NOT
/// regenerate it.
#[test]
fn lsp_did_open_skips_commit_diff_refresh() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    // Open a commit-diff via the command (file mode).
    let diff_uri = session
        .execute_command_with_args("badjuju.diff.commit", serde_json::json!(["@"]))["result"]
        .as_str()
        .unwrap()
        .to_string();
    let path = diff_uri.strip_prefix("file://").unwrap();
    // Drop any preopen mark for the commit-diff URI.
    session.notify(
        "textDocument/didClose",
        serde_json::json!({ "textDocument": { "uri": &diff_uri } }),
    );
    // Corrupt the on-disk content. If cold-open were to refresh, an applyEdit
    // would fire — but the commit URI is pinned, so it must not.
    std::fs::write(path, "STALE COMMIT DIFF\n").unwrap();

    session.did_open(&diff_uri, "jujutsu", "STALE COMMIT DIFF\n");

    let msg =
        session.recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(1500));
    assert!(
        msg.is_none(),
        "commit-diff cold-open must not regenerate, got: {msg:?}"
    );
}

/// Squash window URIs (`.jj/badjuju/squash/<from>-<into>.jujutsu`) can't be
/// reconstructed from filename alone — cold-open must skip them.
#[test]
fn lsp_did_open_skips_squash_window() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    // Hand-fabricate a plausible squash URI; the server should not regen it.
    let squash_path = dir
        .path()
        .join(".jj")
        .join("badjuju")
        .join("squash")
        .join("abcdef123456-fedcba654321.jujutsu");
    std::fs::create_dir_all(squash_path.parent().unwrap()).unwrap();
    std::fs::write(&squash_path, "stale squash\n").unwrap();
    let squash_uri = format!("file://{}", squash_path.display());

    session.did_open(&squash_uri, "jujutsu", "stale squash\n");

    let msg =
        session.recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(1500));
    assert!(
        msg.is_none(),
        "squash-window cold-open must not regenerate, got: {msg:?}"
    );
}

/// `describe.jujutsu` is user-editable; cold-open must never touch it.
#[test]
fn lsp_did_open_does_not_touch_describe() {
    let dir = tempfile::tempdir().unwrap();
    init_jj_repo(dir.path());
    let root_uri = format!("file://{}", dir.path().display());
    let mut session = LspSession::start(dir.path());
    session.initialize(&root_uri);

    let describe_path = dir
        .path()
        .join(".jj")
        .join("badjuju")
        .join("describe.jujutsu");
    std::fs::create_dir_all(describe_path.parent().unwrap()).unwrap();
    std::fs::write(&describe_path, "user draft\n").unwrap();
    let describe_uri = format!("file://{}", describe_path.display());

    session.did_open(&describe_uri, "jujutsu", "user draft\n");

    let msg =
        session.recv_server_request_timeout("workspace/applyEdit", Duration::from_millis(1500));
    assert!(
        msg.is_none(),
        "describe.jujutsu cold-open must never trigger applyEdit, got: {msg:?}"
    );
}
