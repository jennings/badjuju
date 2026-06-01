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
    assert_eq!(
        actions.len(),
        expected.len(),
        "expected {} actions, got {}: {actions:?}",
        expected.len(),
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
    assert_eq!(
        actions.len(),
        7,
        "expected 7 commit actions, got: {actions:?}"
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
