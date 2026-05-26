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

    fn initialize(&mut self, root_uri: &str) -> serde_json::Value {
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
    assert!(content.contains("STATUS:"));
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
        let Some(first) = line.chars().next() else {
            continue;
        };
        if !matches!(first, 'M' | 'A' | 'D' | 'C' | 'R') {
            continue;
        }
        if line[1..].trim() == filename {
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
    assert!(new_status.contains("STATUS:"));
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
    assert!(new_status.contains("STATUS:"));
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
    assert!(new_status.contains("STATUS:"));
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

    // Find a blank line in the buffer (always present after STATUS: header).
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
        content.contains("STATUS:"),
        "status.jujutsu missing STATUS:"
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
        target_uri.ends_with("diff.jujutsu"),
        "expected diff.jujutsu URI, got: {target_uri}"
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
