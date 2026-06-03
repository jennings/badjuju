use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::{Url, *};
use tower_lsp::{Client, LanguageServer};
use tracing::{info, warn};

/// LSP 3.18 `workspace/textDocumentContent` request params (lsp-types 0.94 predates this).
#[derive(serde::Deserialize)]
pub struct TextDocumentContentParams {
    uri: String,
}

/// LSP 3.18 `workspace/textDocumentContent` response.
#[derive(serde::Serialize)]
pub struct TextDocumentContentResult {
    text: String,
}

/// Params for `workspace/textDocumentContent/refresh` server→client notification.
#[derive(serde::Serialize, serde::Deserialize)]
struct TextDocumentContentRefreshParams {
    uri: String,
}

/// Parsed `badjuju-diff:` URI variant.
#[derive(Debug, PartialEq, Eq)]
enum DiffUriKind<'a> {
    Change(&'a str),
    Commit(&'a str),
}

/// Parse a `badjuju-diff:` URI. Accepts both the canonical three-slash form
/// (`badjuju-diff:///change/<id>`) and the one-slash form
/// (`badjuju-diff:/change/<id>`); VS Code's `Uri.toString()` normalizes
/// empty-authority URIs to the latter, so the client may send either.
fn parse_diff_uri(uri: &str) -> std::result::Result<DiffUriKind<'_>, String> {
    let path = uri
        .strip_prefix("badjuju-diff:///")
        .or_else(|| uri.strip_prefix("badjuju-diff:/"))
        .ok_or_else(|| format!("unsupported URI scheme: {uri}"))?;

    if let Some(id) = path.strip_prefix("change/") {
        if id.is_empty() {
            return Err("empty change-id in URI".to_string());
        }
        Ok(DiffUriKind::Change(id))
    } else if let Some(id) = path.strip_prefix("commit/") {
        if id.is_empty() {
            return Err("empty commit-id in URI".to_string());
        }
        Ok(DiffUriKind::Commit(id))
    } else {
        Err(format!("unrecognized badjuju-diff path: {path}"))
    }
}

/// Custom notification: `workspace/textDocumentContent/refresh`.
enum WorkspaceTextDocumentContentRefresh {}
impl tower_lsp::lsp_types::notification::Notification for WorkspaceTextDocumentContentRefresh {
    type Params = TextDocumentContentRefreshParams;
    const METHOD: &'static str = "workspace/textDocumentContent/refresh";
}

use crate::commands::{self, CommandReference, DiffTarget};
use crate::cursor::{self, BufferKind};
use crate::highlighting;
use crate::jj::Jj;
use crate::keymap::{self, KeymapProfile};
use crate::workspace::find_workspace_root;

pub const COMMANDS: &[&str] = &[
    "badjuju.status",
    "badjuju.log",
    "badjuju.describe",
    "badjuju.diff",
    "badjuju.diff.commit",
    "badjuju.new",
    "badjuju.next",
    "badjuju.prev",
    "badjuju.refresh",
    "badjuju.squash",
    "badjuju.squash.into",
    "badjuju.squash.commit",
    "badjuju.squash.cancel",
    "badjuju.squash.toggle",
    "badjuju.squash.select_all",
    "badjuju.squash.select_none",
    "badjuju.unsquash",
    "badjuju.undo",
    "badjuju.abandon",
    "badjuju.keymap",
    "badjuju.help",
    "badjuju.version",
    "badjuju.edit",
    "badjuju.fetch",
    "badjuju.push",
    "badjuju.rebase",
    "badjuju.bookmark",
];

#[derive(Debug)]
struct State {
    workspace_root: Option<PathBuf>,
    binary_path: Option<String>,
    /// Active keymap profile — drives COMMAND REFERENCE rendering and the
    /// `badjuju.keymap` / `badjuju.help` responses.
    keymap_profile: KeymapProfile,
    /// Per-buffer COMMAND REFERENCE overrides from the client (escape hatch).
    /// Overrides replace the profile-rendered defaults for individual buffers.
    command_reference: CommandReference,
    /// Latest text content for open documents, keyed by URI string.
    documents: HashMap<String, String>,
    /// URI of the open status.jujutsu buffer, if any. Set by did_open, cleared by did_close.
    open_status_uri: Option<String>,
    /// URI of the open log.jujutsu buffer, if any. Set by did_open, cleared by did_close.
    open_log_uri: Option<String>,
    /// Open diff buffers: URI → DiffTarget (Change or Commit). Used to refresh
    /// change-mode diffs after mutations and to clean up files on close.
    open_diffs: HashMap<String, DiffTarget>,
    /// True when the client declared `workspace.textDocumentContent` capability.
    /// When true, diffs are served as virtual `badjuju-diff://` URIs; otherwise
    /// the server writes physical `diff-{change,commit}-*.jujutsu` files.
    virtual_diffs_enabled: bool,
    /// Full change-id of a pending commit-to-commit squash source, or `None` when
    /// no squash is in progress. Set by `badjuju.squash.commit` and cleared by
    /// `badjuju.squash.cancel`.
    pending_squash_source: Option<String>,
    /// State of the currently open squash window, if any.
    open_squash_window: Option<commands::SquashWindow>,
    /// Op-ids produced by bad-juju's own mutations, with the time they were recorded.
    /// Used by an op-head watcher to suppress double-refreshes.
    self_caused_ops: HashMap<String, Instant>,
    /// URIs the server pre-wrote and expects the client to open next. When a
    /// `did_open` arrives for one of these, the file is fresh — skip the
    /// cold-open regen path. Inserted by `badjuju.status` / `badjuju.log` /
    /// `badjuju.diff` (change-mode, file://) command handlers; consumed by
    /// `did_open`, defensively cleared by `did_close`.
    preopen_marks: HashSet<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            workspace_root: None,
            binary_path: None,
            keymap_profile: KeymapProfile::Magit,
            command_reference: CommandReference::default(),
            documents: HashMap::new(),
            open_status_uri: None,
            open_log_uri: None,
            open_diffs: HashMap::new(),
            virtual_diffs_enabled: false,
            pending_squash_source: None,
            open_squash_window: None,
            self_caused_ops: HashMap::new(),
            preopen_marks: HashSet::new(),
        }
    }
}

impl State {
    fn jj(&self) -> Option<Jj> {
        let root = self.workspace_root.as_ref()?;
        Some(
            Jj::with_binary_or_default(self.binary_path.as_deref(), root)
                .with_command_reference(self.command_reference.clone()),
        )
    }

    fn record_self_caused_op(&mut self, op_id: String) {
        let cutoff = Instant::now() - Duration::from_secs(10);
        self.self_caused_ops.retain(|_, t| *t > cutoff);
        self.self_caused_ops.insert(op_id, Instant::now());
    }

    fn take_if_self_caused(&mut self, op_id: &str) -> bool {
        let cutoff = Instant::now() - Duration::from_secs(10);
        self.self_caused_ops.retain(|_, t| *t > cutoff);
        self.self_caused_ops.remove(op_id).is_some()
    }
}

#[derive(Debug)]
pub struct Backend {
    client: Client,
    state: Arc<RwLock<State>>,
    shutdown: Arc<tokio::sync::Notify>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            state: Arc::new(RwLock::new(State::default())),
            shutdown: Arc::new(tokio::sync::Notify::new()),
        }
    }

    async fn record_self_caused_op(&self, jj: &Jj) {
        if let Ok(op) = jj.op_head_id() {
            self.state.write().await.record_self_caused_op(op);
        }
    }

    async fn refresh_open_diffs(&self, jj: &Jj, workspace: &std::path::Path) {
        let (open_diffs, virtual_diffs_enabled) = {
            let state = self.state.read().await;
            (state.open_diffs.clone(), state.virtual_diffs_enabled)
        };
        if virtual_diffs_enabled {
            for (uri, target) in &open_diffs {
                if matches!(target, DiffTarget::Change(_)) {
                    self.client
                        .send_notification::<WorkspaceTextDocumentContentRefresh>(
                            TextDocumentContentRefreshParams { uri: uri.clone() },
                        )
                        .await;
                }
            }
        } else {
            let refreshed = commands::refresh_change_diffs(jj, workspace, &open_diffs);
            for (uri, content) in refreshed {
                apply_edit_if_open(&self.client, &self.state, &uri, &content).await;
            }
        }
    }

    pub async fn refresh_open_artifacts(&self, jj: &Jj, workspace: &std::path::Path) {
        let (
            open_status_uri,
            open_log_uri,
            virtual_diffs_enabled,
            pending_squash,
            open_squash_window,
        ) = {
            let state = self.state.read().await;
            (
                state.open_status_uri.clone(),
                state.open_log_uri.clone(),
                state.virtual_diffs_enabled,
                state.pending_squash_source.clone(),
                state.open_squash_window.clone(),
            )
        };
        let pending = pending_squash.as_deref();
        if open_status_uri.is_some() {
            match commands::run_status_with_content(jj, workspace, pending) {
                Ok((uri, content)) => {
                    if !virtual_diffs_enabled {
                        apply_edit_if_open(&self.client, &self.state, &uri, &content).await;
                    }
                }
                Err(e) => {
                    self.client
                        .log_message(MessageType::WARNING, format!("refresh status failed: {e}"))
                        .await;
                }
            }
        }
        if open_log_uri.is_some() {
            match commands::regenerate_log_if_present_with_content(jj, workspace, pending) {
                Ok(Some((uri, content))) => {
                    if !virtual_diffs_enabled {
                        apply_edit_if_open(&self.client, &self.state, &uri, &content).await;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.client
                        .log_message(MessageType::WARNING, format!("refresh log failed: {e}"))
                        .await;
                }
            }
        }
        self.refresh_open_diffs(jj, workspace).await;
        if let Some(window) = open_squash_window {
            match commands::regenerate_squash_window(jj, &window) {
                Ok((uri, content)) => {
                    apply_edit_if_open(&self.client, &self.state, &uri, &content).await;
                    self.state.write().await.documents.insert(uri, content);
                }
                Err(e) => {
                    self.client
                        .log_message(MessageType::WARNING, format!("refresh squash failed: {e}"))
                        .await;
                }
            }
        }
    }

    /// Handler for `workspace/textDocumentContent` (LSP 3.18).
    /// Serves content for `badjuju-diff:///change/<id>` and `badjuju-diff:///commit/<id>` URIs.
    pub async fn text_document_content(
        &self,
        params: TextDocumentContentParams,
    ) -> Result<TextDocumentContentResult> {
        let uri = &params.uri;
        let state = self.state.read().await;
        let jj = state.jj().ok_or_else(Error::invalid_request)?;
        drop(state);

        let kind = parse_diff_uri(uri).map_err(lsp_err)?;
        let text = match kind {
            DiffUriKind::Change(id) => {
                commands::diff_content_for_change(&jj, id).map_err(lsp_err)?
            }
            DiffUriKind::Commit(id) => {
                commands::diff_content_for_commit(&jj, id).map_err(lsp_err)?
            }
        };

        Ok(TextDocumentContentResult { text })
    }
}

/// Send a `workspace/applyEdit` with full-document replacement to the client
/// when `uri` is currently open. This is the file-based fallback for clients
/// (notably Helix) that don't auto-reload buffers when the underlying file
/// changes on disk and don't support `workspace/textDocumentContent`.
///
/// `describe.jujutsu` URIs are skipped as defense in depth — the refresh
/// callers never write describe buffers, but it would be wrong to overwrite
/// the user's in-progress edits if they did.
async fn apply_edit_if_open(client: &Client, state: &Arc<RwLock<State>>, uri: &str, content: &str) {
    if uri.ends_with("/describe.jujutsu") {
        return;
    }
    let is_open = state.read().await.documents.contains_key(uri);
    if !is_open {
        return;
    }
    let Ok(uri_url) = Url::parse(uri) else {
        return;
    };
    let mut changes = HashMap::new();
    changes.insert(
        uri_url,
        vec![TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: u32::MAX,
                    character: u32::MAX,
                },
            },
            new_text: content.to_string(),
        }],
    );
    let _ = client
        .apply_edit(WorkspaceEdit {
            changes: Some(changes),
            ..Default::default()
        })
        .await;
}

/// Spawns a debounced watcher on `.jj/repo/op_heads/heads/`. On each external
/// op-head change (i.e. not caused by bad-juju itself), refreshes all open
/// status, log, and change-diff buffers.
fn spawn_op_head_watcher(
    heads_dir: PathBuf,
    state: Arc<RwLock<State>>,
    client: Client,
    shutdown: Arc<tokio::sync::Notify>,
) {
    use notify_debouncer_mini::notify::RecursiveMode;
    use notify_debouncer_mini::{DebounceEventResult, new_debouncer};

    let watcher_fired = Arc::new(tokio::sync::Notify::new());
    let watcher_fired2 = Arc::clone(&watcher_fired);

    let mut debouncer = match new_debouncer(
        Duration::from_millis(500),
        move |res: DebounceEventResult| {
            if res.is_ok() {
                watcher_fired2.notify_one();
            }
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            warn!("op-head watcher: failed to create debouncer: {e}");
            return;
        }
    };

    if let Err(e) = debouncer
        .watcher()
        .watch(&heads_dir, RecursiveMode::NonRecursive)
    {
        warn!(
            "op-head watcher: failed to watch {}: {e}",
            heads_dir.display()
        );
        return;
    }

    // Keep the debouncer alive in a thread; exit when the async task signals done.
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let _debouncer = debouncer;
        let _ = stop_rx.recv();
    });

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = watcher_fired.notified() => {
                    let (jj, workspace) = {
                        let s = state.read().await;
                        match (s.jj(), s.workspace_root.clone()) {
                            (Some(jj), Some(root)) => (jj, root),
                            _ => continue,
                        }
                    };
                    let op_id = match jj.op_head_id() {
                        Ok(id) => id,
                        Err(_) => continue,
                    };
                    if state.write().await.take_if_self_caused(&op_id) {
                        continue;
                    }
                    let (open_status_uri, open_log_uri, virtual_diffs_enabled, pending_squash) = {
                        let s = state.read().await;
                        (
                            s.open_status_uri.clone(),
                            s.open_log_uri.clone(),
                            s.virtual_diffs_enabled,
                            s.pending_squash_source.clone(),
                        )
                    };
                    let pending = pending_squash.as_deref();
                    if open_status_uri.is_some() {
                        match commands::run_status_with_content(&jj, &workspace, pending) {
                            Ok((uri, content)) => {
                                if !virtual_diffs_enabled {
                                    apply_edit_if_open(&client, &state, &uri, &content).await;
                                }
                            }
                            Err(e) => {
                                client
                                    .log_message(
                                        MessageType::WARNING,
                                        format!("watcher: refresh status failed: {e}"),
                                    )
                                    .await;
                            }
                        }
                    }
                    if open_log_uri.is_some() {
                        match commands::regenerate_log_if_present_with_content(&jj, &workspace, pending) {
                            Ok(Some((uri, content))) => {
                                if !virtual_diffs_enabled {
                                    apply_edit_if_open(&client, &state, &uri, &content).await;
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                client
                                    .log_message(
                                        MessageType::WARNING,
                                        format!("watcher: refresh log failed: {e}"),
                                    )
                                    .await;
                            }
                        }
                    }
                    let open_diffs = state.read().await.open_diffs.clone();
                    if virtual_diffs_enabled {
                        for (uri, target) in &open_diffs {
                            if matches!(target, DiffTarget::Change(_)) {
                                client
                                    .send_notification::<WorkspaceTextDocumentContentRefresh>(
                                        TextDocumentContentRefreshParams { uri: uri.clone() },
                                    )
                                    .await;
                            }
                        }
                    } else {
                        let refreshed =
                            commands::refresh_change_diffs(&jj, &workspace, &open_diffs);
                        for (uri, content) in refreshed {
                            apply_edit_if_open(&client, &state, &uri, &content).await;
                        }
                    }
                    // Refresh open squash window if any.
                    let open_squash_window = state.read().await.open_squash_window.clone();
                    if let Some(window) = open_squash_window {
                        match commands::regenerate_squash_window(&jj, &window) {
                            Ok((uri, content)) => {
                                apply_edit_if_open(&client, &state, &uri, &content).await;
                                state.write().await.documents.insert(uri, content);
                            }
                            Err(e) => {
                                client
                                    .log_message(
                                        MessageType::WARNING,
                                        format!("watcher: refresh squash failed: {e}"),
                                    )
                                    .await;
                            }
                        }
                    }
                }
                _ = shutdown.notified() => {
                    let _ = stop_tx.send(());
                    break;
                }
            }
        }
    });
}

fn lsp_err(msg: impl ToString) -> Error {
    let mut err = Error::internal_error();
    err.message = msg.to_string().into();
    err
}

/// Read a `file://` URI from disk; returns `None` for non-`file:` schemes or
/// missing files. Used as a fallback when a command's cursor-form arg refers
/// to a buffer the server doesn't have cached in `State.documents`.
fn read_uri_from_disk(uri: &str) -> Option<String> {
    let url = Url::parse(uri).ok()?;
    let path = url.to_file_path().ok()?;
    std::fs::read_to_string(path).ok()
}

/// Resolve `(file, revision)` for squash/unsquash. Accepts either the legacy
/// `[file_str, revision_str]` form (Neovim CLI) or a single
/// `[{cursor:{uri,line}}]` argument (code actions). For cursor form, both
/// file and revision are read from the same status.jujutsu line.
fn resolve_file_scoped_args(
    first: Option<&serde_json::Value>,
    second: Option<&serde_json::Value>,
    documents: &HashMap<String, String>,
) -> Result<(String, String)> {
    commands::resolve_file_and_revision_arg(first, second, |uri| {
        documents
            .get(uri)
            .cloned()
            .or_else(|| read_uri_from_disk(uri))
    })
    .map_err(lsp_err)
}

/// Build the seven code actions offered for a commit line (log buffer commit
/// headers; reused for status buffer commit headers in T7). Direct-action
/// commands invoke server commands with a pre-resolved revision; prompt-needing
/// commands invoke client-registered handlers (`badjuju.client.*Prompt`).
fn commit_actions(revision: &str) -> Vec<CodeActionOrCommand> {
    let arg = serde_json::Value::String(revision.to_string());
    let make = |title: String, command: &str| -> CodeActionOrCommand {
        CodeActionOrCommand::CodeAction(CodeAction {
            title,
            kind: Some(CodeActionKind::EMPTY),
            command: Some(Command {
                title: String::new(),
                command: command.to_string(),
                arguments: Some(vec![arg.clone()]),
            }),
            ..Default::default()
        })
    };
    vec![
        make(format!("Edit commit {revision}"), "badjuju.edit"),
        make(format!("Abandon commit {revision}"), "badjuju.abandon"),
        make(format!("Describe commit {revision}"), "badjuju.describe"),
        make(format!("Show diff for {revision}"), "badjuju.diff"),
        make(format!("New child of {revision}"), "badjuju.new"),
        make(
            format!("Rebase commit {revision}…"),
            "badjuju.client.rebasePrompt",
        ),
        make(
            format!("Bookmark {revision}…"),
            "badjuju.client.bookmarkPrompt",
        ),
    ]
}

/// Build the single "Apply revset" code action for a log-buffer shortcut line.
/// The command ships a cursor-form arg so the server re-reads the shortcut
/// when the user picks the action (stable across buffer regenerations).
fn log_shortcut_action(uri: &str, line: usize, label: &str) -> CodeActionOrCommand {
    let cursor_arg = serde_json::json!({ "cursor": { "uri": uri, "line": line } });
    CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Apply revset: {label}"),
        kind: Some(CodeActionKind::EMPTY),
        command: Some(Command {
            title: String::new(),
            command: "badjuju.log".to_string(),
            arguments: Some(vec![cursor_arg]),
        }),
        ..Default::default()
    })
}

/// Build code actions offered for a squash window buffer line.
///
/// - File-path line in REMAINING: "Move file to SELECTED"
/// - File-path line in SELECTED: "Move file to REMAINING"
/// - Hunk line in REMAINING: "Move hunk to SELECTED"
/// - Hunk line in SELECTED: "Move hunk to REMAINING"
/// - Anywhere: "Move all to SELECTED" (when REMAINING non-empty)
/// - Anywhere: "Move all to REMAINING" (when SELECTED non-empty)
fn squash_window_actions(uri: &str, line: usize, content: &str) -> Vec<CodeActionOrCommand> {
    let cursor_arg = serde_json::json!({ "cursor": { "uri": uri, "line": line } });
    let make =
        |title: String, command: &str, args: Vec<serde_json::Value>| -> CodeActionOrCommand {
            CodeActionOrCommand::CodeAction(CodeAction {
                title,
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: String::new(),
                    command: command.to_string(),
                    arguments: Some(args),
                }),
                ..Default::default()
            })
        };

    let mut actions: Vec<CodeActionOrCommand> = Vec::new();
    let section = cursor::squash_section_at_line(content, line);

    // File or hunk actions
    if let Some(sec) = section {
        let is_file_only = cursor::squash_file_at_line(content, line).is_some()
            && cursor::squash_hunk_at_line(content, line).is_none();
        let is_hunk = cursor::squash_hunk_at_line(content, line).is_some();

        if let Some(file) = cursor::squash_file_at_line(content, line)
            && is_file_only
        {
            let title = match sec {
                cursor::SquashSection::Remaining => {
                    format!("Move file {file} to SELECTED")
                }
                cursor::SquashSection::Selected => {
                    format!("Move file {file} to REMAINING")
                }
            };
            actions.push(make(
                title,
                "badjuju.squash.toggle",
                vec![cursor_arg.clone()],
            ));
        }
        if is_hunk {
            let title = match sec {
                cursor::SquashSection::Remaining => "Move hunk to SELECTED".to_string(),
                cursor::SquashSection::Selected => "Move hunk to REMAINING".to_string(),
            };
            actions.push(make(
                title,
                "badjuju.squash.toggle",
                vec![cursor_arg.clone()],
            ));
        }
    }

    // Bulk actions — check whether REMAINING / SELECTED sections have content.
    let lines: Vec<&str> = content.lines().collect();
    let remaining_has_content = lines.iter().enumerate().any(|(i, _)| {
        cursor::squash_section_at_line(content, i) == Some(cursor::SquashSection::Remaining)
            && cursor::squash_file_at_line(content, i).is_some()
    });
    let selected_has_content = lines.iter().enumerate().any(|(i, _)| {
        cursor::squash_section_at_line(content, i) == Some(cursor::SquashSection::Selected)
            && cursor::squash_file_at_line(content, i).is_some()
    });

    if remaining_has_content {
        actions.push(make(
            "Move all hunks to SELECTED".to_string(),
            "badjuju.squash.select_all",
            vec![],
        ));
    }
    if selected_has_content {
        actions.push(make(
            "Move all hunks to REMAINING".to_string(),
            "badjuju.squash.select_none",
            vec![],
        ));
    }

    actions
}

/// Build the squash-flow code actions offered for a commit-header row in status
/// and log buffers. When no squash is pending, offers "Squash from this
/// revision"; when a squash is pending, offers "Squash into this revision"
/// and "Cancel pending squash". Both use cursor-form args for stability
/// across buffer regenerations.
fn squash_commit_actions(
    uri: &str,
    line: usize,
    pending_source: Option<&str>,
) -> Vec<CodeActionOrCommand> {
    let cursor_arg = serde_json::json!({ "cursor": { "uri": uri, "line": line } });
    let make =
        |title: String, command: &str, args: Vec<serde_json::Value>| -> CodeActionOrCommand {
            CodeActionOrCommand::CodeAction(CodeAction {
                title,
                kind: Some(CodeActionKind::EMPTY),
                command: Some(Command {
                    title: String::new(),
                    command: command.to_string(),
                    arguments: Some(args),
                }),
                ..Default::default()
            })
        };
    if pending_source.is_some() {
        vec![
            make(
                "Squash into this revision".to_string(),
                "badjuju.squash.commit",
                vec![cursor_arg],
            ),
            make(
                "Cancel pending squash".to_string(),
                "badjuju.squash.cancel",
                vec![],
            ),
        ]
    } else {
        vec![make(
            "Squash from this revision".to_string(),
            "badjuju.squash.commit",
            vec![cursor_arg],
        )]
    }
}

/// Build the two squash/unsquash code actions offered for a status-buffer file
/// line. Both commands ship a cursor-form arg so the server resolves the file
/// and revision fresh when the user picks the action (stable across buffer
/// regenerations).
fn file_actions(uri: &str, line: usize, file: &str) -> Vec<CodeActionOrCommand> {
    let cursor_arg = serde_json::json!({ "cursor": { "uri": uri, "line": line } });
    let make = |title: String, command: &str| -> CodeActionOrCommand {
        CodeActionOrCommand::CodeAction(CodeAction {
            title,
            kind: Some(CodeActionKind::EMPTY),
            command: Some(Command {
                title: String::new(),
                command: command.to_string(),
                arguments: Some(vec![cursor_arg.clone()]),
            }),
            ..Default::default()
        })
    };
    vec![
        make(format!("Squash {file}"), "badjuju.squash"),
        make(format!("Unsquash {file}"), "badjuju.unsquash"),
    ]
}

/// Parse the optional `commandReference` object passed in `initializationOptions`.
/// Each of `status`, `log`, and `diff` is an optional string override. Missing
/// or non-string values fall back to the profile-rendered defaults.
fn parse_command_reference(value: &serde_json::Value, profile: &KeymapProfile) -> CommandReference {
    let pick = |key: &str| -> Option<String> {
        value
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    CommandReference::with_profile(
        profile,
        pick("status"),
        pick("log"),
        pick("diff"),
        pick("squash"),
    )
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let binary_path = params
            .initialization_options
            .as_ref()
            .and_then(|o| o.get("binaryPath"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let keymap_profile = params
            .initialization_options
            .as_ref()
            .and_then(|o| o.get("keymapProfile"))
            .and_then(|v| v.as_str())
            .and_then(|s| {
                let p = s.parse::<KeymapProfile>().ok();
                if p.is_none() {
                    warn!(profile = s, "unknown keymapProfile; falling back to magit");
                }
                p
            })
            .unwrap_or_default();

        let command_reference = params
            .initialization_options
            .as_ref()
            .and_then(|o| o.get("commandReference"))
            .map(|v| parse_command_reference(v, &keymap_profile))
            .unwrap_or_else(|| CommandReference::from_profile(&keymap_profile));

        let search_start = params
            .root_uri
            .as_ref()
            .and_then(|u| u.to_file_path().ok())
            .or_else(|| {
                params
                    .workspace_folders
                    .as_deref()
                    .and_then(|f| f.first())
                    .and_then(|f| f.uri.to_file_path().ok())
            });

        let workspace_root = search_start.as_deref().and_then(find_workspace_root);

        // Detect LSP 3.18 workspace/textDocumentContent client capability.
        // Check both `params.capabilities.workspace.textDocumentContent` (spec-compliant,
        // for clients with native support) and `initializationOptions.virtualDiffs`
        // (escape hatch for clients like VS Code that use vscode-languageclient 9
        // and need to opt in via initializationOptions instead).
        let via_capabilities = serde_json::to_value(&params.capabilities)
            .ok()
            .and_then(|v| {
                v.get("workspace")
                    .and_then(|w| w.get("textDocumentContent"))
                    .map(|f| !f.is_null())
            })
            .unwrap_or(false);
        let via_options = params
            .initialization_options
            .as_ref()
            .and_then(|o| o.get("virtualDiffs"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let virtual_diffs_enabled = via_capabilities || via_options;

        {
            let mut state = self.state.write().await;
            state.binary_path = binary_path;
            state.keymap_profile = keymap_profile;
            state.command_reference = command_reference;
            state.workspace_root = workspace_root.clone();
            state.virtual_diffs_enabled = virtual_diffs_enabled;
        }

        if let Some(ref root) = workspace_root {
            info!(root = %root.display(), "found jj workspace");
            commands::sweep_stale_diff_files(root);
            let heads_dir = root.join(".jj/repo/op_heads/heads");
            if heads_dir.exists() {
                spawn_op_head_watcher(
                    heads_dir,
                    Arc::clone(&self.state),
                    self.client.clone(),
                    Arc::clone(&self.shutdown),
                );
            }
        } else {
            warn!("no jj workspace found; commands will return errors until a workspace is opened");
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..Default::default()
                    },
                )),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: COMMANDS.iter().map(|s| s.to_string()).collect(),
                    work_done_progress_options: Default::default(),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: highlighting::TOKEN_TYPES.to_vec(),
                                token_modifiers: highlighting::TOKEN_MODIFIERS.to_vec(),
                            },
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        let Some(kind) = BufferKind::from_uri(&uri) else {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };
        let content = {
            let state = self.state.read().await;
            state
                .documents
                .get(&uri)
                .cloned()
                .or_else(|| read_uri_from_disk(&uri))
        };
        let Some(content) = content else {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };
        let data = highlighting::semantic_tokens(&content, kind);
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data,
        })))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.to_string();
        let line = params.range.start.line as usize;

        let Some(kind) = BufferKind::from_uri(&uri) else {
            return Ok(Some(vec![]));
        };

        let (content, pending_squash) = {
            let state = self.state.read().await;
            let content = state
                .documents
                .get(&uri)
                .cloned()
                .or_else(|| read_uri_from_disk(&uri));
            let pending = state.pending_squash_source.clone();
            (content, pending)
        };
        let Some(content) = content else {
            return Ok(Some(vec![]));
        };

        let mut actions: Vec<CodeActionOrCommand> = Vec::new();

        match kind {
            BufferKind::Log => {
                // Try the shortcut form first — shortcut lines start with
                // `JJ:` and don't match commit-header detection, so the order
                // is for clarity rather than correctness.
                if let Some(shortcut) = cursor::log_shortcut_at_line(&content, line) {
                    actions.push(log_shortcut_action(&uri, line, &shortcut.label));
                } else if let Some(revision) = cursor::revision_at_line(&content, line, kind) {
                    actions.extend(commit_actions(&revision));
                    // Add squash actions when cursor is on a commit header row.
                    if cursor::commit_id_at_line(&content, line).is_some() {
                        actions.extend(squash_commit_actions(
                            &uri,
                            line,
                            pending_squash.as_deref(),
                        ));
                    }
                }
            }
            BufferKind::Status => {
                // File lines take precedence — squash/unsquash actions reference
                // the file under the cursor. The server re-resolves the file
                // when the command runs, so the action stays stable across
                // buffer regenerations.
                if let Some(file) = cursor::file_at_line(&content, line) {
                    actions.extend(file_actions(&uri, line, &file));
                } else if let Some(revision) = cursor::commit_id_at_line(&content, line) {
                    actions.extend(commit_actions(&revision));
                    actions.extend(squash_commit_actions(&uri, line, pending_squash.as_deref()));
                }
            }
            BufferKind::Squash => {
                actions.extend(squash_window_actions(&uri, line, &content));
            }
            BufferKind::Diff => {}
        }

        Ok(Some(actions))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let line = params.text_document_position_params.position.line as usize;

        let Some(kind) = BufferKind::from_uri(&uri) else {
            return Ok(None);
        };
        if matches!(kind, BufferKind::Diff) {
            return Ok(None);
        }

        let content = {
            let state = self.state.read().await;
            state
                .documents
                .get(&uri)
                .cloned()
                .or_else(|| read_uri_from_disk(&uri))
        };
        let Some(content) = content else {
            return Ok(None);
        };

        let Some(revision) = cursor::revision_at_line(&content, line, kind) else {
            return Ok(None);
        };

        let (jj, workspace, virtual_diffs_enabled) = {
            let state = self.state.read().await;
            match (state.jj(), state.workspace_root.clone()) {
                (Some(jj), Some(root)) => (jj, root, state.virtual_diffs_enabled),
                _ => return Ok(None),
            }
        };

        let result = if virtual_diffs_enabled {
            commands::run_diff_change_virtual(&jj, &revision)
        } else {
            commands::run_diff_change(&jj, &workspace, &revision)
        };
        match result {
            Ok((diff_uri, _)) => {
                let target_uri = Url::parse(&diff_uri).map_err(|_| lsp_err("bad diff URI"))?;
                let location = Location {
                    uri: target_uri,
                    range: Range::default(),
                };
                Ok(Some(GotoDefinitionResponse::Scalar(location)))
            }
            Err(_) => Ok(None),
        }
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri.to_string();

        let kind = BufferKind::from_uri(&uri);
        if !matches!(kind, Some(BufferKind::Status | BufferKind::Squash)) {
            return Ok(None);
        }

        let content = {
            let state = self.state.read().await;
            state
                .documents
                .get(&uri)
                .cloned()
                .or_else(|| read_uri_from_disk(&uri))
        };
        let Some(content) = content else {
            return Ok(None);
        };

        let ranges = match kind {
            Some(BufferKind::Squash) => commands::squash_folding_ranges(&content),
            _ => commands::status_folding_ranges(&content),
        };
        Ok(Some(ranges))
    }

    async fn initialized(&self, _: InitializedParams) {
        info!("server initialized");
        self.client
            .log_message(MessageType::INFO, "badjuju server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        self.shutdown.notify_waiters();
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let text = params.text_document.text;
        let preopened = {
            let mut state = self.state.write().await;
            state.documents.insert(uri.clone(), text.clone());
            if uri.ends_with("/status.jujutsu") {
                state.open_status_uri = Some(uri.clone());
            } else if uri.ends_with("/log.jujutsu") {
                state.open_log_uri = Some(uri.clone());
            }
            state.preopen_marks.remove(&uri)
        };

        // If the server just pre-wrote this file (via badjuju.status / .log /
        // .diff), the content on disk is already fresh — skip cold-open regen.
        if preopened {
            return;
        }

        let Some(kind) = BufferKind::from_uri(&uri) else {
            return;
        };

        // Cold open: regenerate by kind. Squash windows can't be reconstructed
        // from filename alone, and diff-commit URIs are pinned by design.
        let (jj, workspace) = {
            let state = self.state.read().await;
            match (state.jj(), state.workspace_root.clone()) {
                (Some(jj), Some(root)) => (jj, root),
                _ => return,
            }
        };

        let (pending_squash, virtual_diffs_enabled) = {
            let state = self.state.read().await;
            (
                state.pending_squash_source.clone(),
                state.virtual_diffs_enabled,
            )
        };
        let pending = pending_squash.as_deref();

        let regen: std::result::Result<Option<(String, String)>, commands::CommandError> =
            match kind {
                BufferKind::Status => {
                    commands::run_status_with_content(&jj, &workspace, pending).map(Some)
                }
                BufferKind::Log => {
                    commands::run_log_with_content(&jj, &workspace, "", pending).map(Some)
                }
                BufferKind::Diff if !virtual_diffs_enabled => {
                    if let Some(change_id) = commands::parse_change_id_from_uri(&uri) {
                        commands::run_diff_change_with_content(&jj, &workspace, &change_id)
                            .map(|(uri, _, content)| Some((uri, content)))
                    } else {
                        // diff-commit-* is immutable; legacy diff.jujutsu is not
                        // refreshed on cold open (no id encoded in the filename).
                        Ok(None)
                    }
                }
                BufferKind::Diff | BufferKind::Squash => Ok(None),
            };

        match regen {
            Ok(Some((regen_uri, content))) => {
                // Skip applyEdit when the regen matches what the client just
                // reported — avoids Helix's modified-indicator flash for files
                // the CLI wrote fresh moments ago.
                if content != text {
                    apply_edit_if_open(&self.client, &self.state, &regen_uri, &content).await;
                }
            }
            Ok(None) => {}
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("did_open cold-open refresh failed: {e}"),
                    )
                    .await;
            }
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let Some(change) = params.content_changes.into_iter().next() {
            self.state.write().await.documents.insert(uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let mut state = self.state.write().await;
        state.documents.remove(&uri);
        // Defensively drop any pre-open mark so a stale entry doesn't suppress
        // the next cold-open regen.
        state.preopen_marks.remove(&uri);
        if state.open_status_uri.as_deref() == Some(&uri) {
            state.open_status_uri = None;
        }
        if state.open_log_uri.as_deref() == Some(&uri) {
            state.open_log_uri = None;
        }
        if state.open_diffs.remove(&uri).is_some() {
            // Best-effort delete; ignore errors (file may already be gone).
            if let Some(path) = commands::path_from_uri(&uri) {
                let _ = std::fs::remove_file(path);
            }
        }
        if matches!(BufferKind::from_uri(&uri), Some(BufferKind::Squash)) {
            if state
                .open_squash_window
                .as_ref()
                .is_some_and(|w| w.uri == uri)
            {
                state.open_squash_window = None;
            }
            if let Some(path) = commands::path_from_uri(&uri) {
                let _ = std::fs::remove_file(path);
            }
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri_str = params.text_document.uri.to_string();

        let state = self.state.read().await;
        let (jj, workspace) = match (state.jj(), state.workspace_root.clone()) {
            (Some(jj), Some(root)) => (jj, root),
            _ => {
                self.client
                    .log_message(MessageType::WARNING, "did_save: no jj workspace")
                    .await;
                return;
            }
        };

        // Get content from params (if include_text worked) or fall back to cached doc.
        let content = params
            .text
            .or_else(|| state.documents.get(&uri_str).cloned());

        let Some(text) = content else {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("did_save: no content for {uri_str}"),
                )
                .await;
            return;
        };

        let filename = params
            .text_document
            .uri
            .to_file_path()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));

        drop(state);

        match filename.as_deref() {
            Some("describe.jujutsu") => {
                if let Err(e) = commands::on_describe_save(&jj, &workspace, &text) {
                    self.client
                        .log_message(MessageType::ERROR, format!("describe save failed: {e}"))
                        .await;
                } else {
                    self.record_self_caused_op(&jj).await;
                    self.refresh_open_diffs(&jj, &workspace).await;
                }
            }
            Some("log.jujutsu") => {
                if let Err(e) = commands::on_log_save(&jj, &workspace, &text) {
                    self.client
                        .log_message(MessageType::ERROR, format!("log save failed: {e}"))
                        .await;
                }
            }
            _ => {}
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        info!(command = %params.command, "execute_command");

        // Keymap commands don't require a workspace root.
        if params.command == "badjuju.keymap" {
            let profile = self.state.read().await.keymap_profile.clone();
            let windows = serde_json::json!({
                "profile": profile.as_str(),
                "windows": {
                    "status": keymap::entries_for_window(&profile, "status"),
                    "log": keymap::entries_for_window(&profile, "log"),
                    "diff": keymap::entries_for_window(&profile, "diff"),
                }
            });
            return Ok(Some(windows));
        }
        if params.command == "badjuju.help" {
            let profile = self.state.read().await.keymap_profile.clone();
            let window = params
                .arguments
                .first()
                .and_then(|v| v.as_str())
                .unwrap_or("status");
            let entries = keymap::entries_for_window(&profile, window);
            return Ok(Some(serde_json::to_value(entries).unwrap_or_default()));
        }
        if params.command == "badjuju.version" {
            return Ok(Some(serde_json::json!({
                "version": env!("BADJUJU_VERSION"),
                "commit": env!("BADJUJU_COMMIT"),
            })));
        }

        let (jj, workspace, documents, virtual_diffs_enabled) = {
            let state = self.state.read().await;
            let jj = state.jj().ok_or_else(Error::invalid_request)?;
            let workspace = state
                .workspace_root
                .clone()
                .ok_or_else(Error::invalid_request)?;
            let documents = state.documents.clone();
            let virtual_diffs_enabled = state.virtual_diffs_enabled;
            (jj, workspace, documents, virtual_diffs_enabled)
        };

        match params.command.as_str() {
            "badjuju.status" => {
                let uri = commands::run_status(&jj, &workspace).map_err(lsp_err)?;
                self.state.write().await.preopen_marks.insert(uri.clone());
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.log" => {
                let arg = params.arguments.first();
                // Cursor-form: read log.jujutsu and pull the revset off a
                // `JJ: <Label>: <revset>` shortcut line under the cursor.
                let cursor_revset = commands::resolve_log_shortcut_arg(arg, |uri| {
                    documents
                        .get(uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(uri))
                })
                .map_err(lsp_err)?;
                let revset = cursor_revset
                    .unwrap_or_else(|| arg.and_then(|v| v.as_str()).unwrap_or("").to_string());
                let uri = commands::run_log(&jj, &workspace, &revset).map_err(lsp_err)?;
                self.state.write().await.preopen_marks.insert(uri.clone());
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.describe" => {
                let revision = commands::resolve_revision_arg(params.arguments.first(), |uri| {
                    documents
                        .get(uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(uri))
                })
                .map_err(lsp_err)?;
                let uri = commands::run_describe(&jj, &workspace, &revision).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.diff" => {
                let revision = commands::resolve_revision_arg(params.arguments.first(), |uri| {
                    documents
                        .get(uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(uri))
                })
                .map_err(lsp_err)?;
                let (uri, target) = if virtual_diffs_enabled {
                    commands::run_diff_change_virtual(&jj, &revision).map_err(lsp_err)?
                } else {
                    commands::run_diff_change(&jj, &workspace, &revision).map_err(lsp_err)?
                };
                {
                    let mut state = self.state.write().await;
                    state.open_diffs.insert(uri.clone(), target);
                    // Only file:// URIs go through did_open; virtual URIs are
                    // fetched via workspace/textDocumentContent.
                    if !virtual_diffs_enabled {
                        state.preopen_marks.insert(uri.clone());
                    }
                }
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.diff.commit" => {
                let revision = commands::resolve_revision_arg(params.arguments.first(), |uri| {
                    documents
                        .get(uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(uri))
                })
                .map_err(lsp_err)?;
                let (uri, target) = if virtual_diffs_enabled {
                    commands::run_diff_commit_virtual(&jj, &revision).map_err(lsp_err)?
                } else {
                    commands::run_diff_commit(&jj, &workspace, &revision).map_err(lsp_err)?
                };
                self.state
                    .write()
                    .await
                    .open_diffs
                    .insert(uri.clone(), target);
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.new" => {
                let parent = commands::resolve_revision_arg(params.arguments.first(), |uri| {
                    documents
                        .get(uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(uri))
                })
                .map_err(lsp_err)?;
                let uri = commands::run_new(&jj, &workspace, &parent).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.next" => {
                let edit = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let uri = commands::run_next(&jj, &workspace, edit).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.prev" => {
                let edit = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let uri = commands::run_prev(&jj, &workspace, edit).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.refresh" => {
                let doc_uri = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri = commands::run_refresh(&jj, &workspace, doc_uri).map_err(lsp_err)?;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.squash" => {
                if let Some(cp) =
                    commands::parse_cursor_arg(params.arguments.first()).map_err(lsp_err)?
                {
                    let content = documents
                        .get(&cp.uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(&cp.uri))
                        .ok_or_else(|| lsp_err(format!("document not found: {}", cp.uri)))?;
                    match cursor::cursor_target_at_line(&content, cp.line) {
                        Some(cursor::CursorTarget::WorkingCopyFile { path }) => {
                            match commands::run_squash_working_copy(&jj, &workspace, &path) {
                                Ok(uri) => {
                                    self.record_self_caused_op(&jj).await;
                                    self.refresh_open_diffs(&jj, &workspace).await;
                                    return Ok(Some(serde_json::Value::String(uri)));
                                }
                                Err(commands::CommandError::RequiresParentSelection {
                                    file,
                                    candidates,
                                }) => {
                                    let mut err = Error::internal_error();
                                    err.message = "squash requires parent selection".into();
                                    err.data = Some(serde_json::json!({
                                        "code": "RequiresParentSelection",
                                        "file": file,
                                        "candidates": candidates.iter()
                                            .map(|(id, label)| {
                                                serde_json::json!({ "id": id, "label": label })
                                            })
                                            .collect::<Vec<_>>()
                                    }));
                                    return Err(err);
                                }
                                Err(e) => return Err(lsp_err(e)),
                            }
                        }
                        Some(cursor::CursorTarget::ParentFile { .. }) => {
                            let uri = commands::write_status(
                                &jj,
                                &workspace,
                                Some(
                                    "squash: cursor on parent change \
                                     — use unsquash (U) instead",
                                ),
                            )
                            .map_err(lsp_err)?;
                            return Ok(Some(serde_json::Value::String(uri)));
                        }
                        _ => {}
                    }
                }
                let (file, revision) = resolve_file_scoped_args(
                    params.arguments.first(),
                    params.arguments.get(1),
                    &documents,
                )?;
                let uri =
                    commands::run_squash(&jj, &workspace, &file, &revision).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.squash.commit" => {
                let already_pending = self.state.read().await.pending_squash_source.clone();

                if let Some(from) = already_pending {
                    // Destination selection: resolve the cursor to a destination change-id.
                    let revision =
                        commands::resolve_revision_arg(params.arguments.first(), |uri| {
                            documents
                                .get(uri)
                                .cloned()
                                .or_else(|| read_uri_from_disk(uri))
                        })
                        .map_err(lsp_err)?;
                    let rev = if revision.is_empty() { "@" } else { &revision };
                    let into = jj.change_id_of(rev).map_err(lsp_err)?;

                    let (squash_uri, window) =
                        commands::run_squash_window(&jj, &workspace, &from, &into)
                            .map_err(lsp_err)?;

                    {
                        let mut state = self.state.write().await;
                        state.pending_squash_source = None;
                        state.open_squash_window = Some(window);
                    }
                    Ok(Some(serde_json::Value::String(squash_uri)))
                } else {
                    // Source selection: resolve the cursor to a source change-id.
                    let revision =
                        commands::resolve_revision_arg(params.arguments.first(), |uri| {
                            documents
                                .get(uri)
                                .cloned()
                                .or_else(|| read_uri_from_disk(uri))
                        })
                        .map_err(lsp_err)?;
                    let rev = if revision.is_empty() { "@" } else { &revision };
                    let change_id = jj.change_id_of(rev).map_err(lsp_err)?;

                    self.state.write().await.pending_squash_source = Some(change_id.clone());

                    let (uri, content) =
                        commands::run_status_with_content(&jj, &workspace, Some(&change_id))
                            .map_err(lsp_err)?;
                    apply_edit_if_open(&self.client, &self.state, &uri, &content).await;
                    if let Ok(Some((log_uri, log_content))) =
                        commands::regenerate_log_if_present_with_content(
                            &jj,
                            &workspace,
                            Some(&change_id),
                        )
                    {
                        apply_edit_if_open(&self.client, &self.state, &log_uri, &log_content).await;
                    }
                    Ok(Some(serde_json::Value::String(uri)))
                }
            }
            "badjuju.squash.cancel" => {
                self.state.write().await.pending_squash_source = None;

                let (uri, content) =
                    commands::run_status_with_content(&jj, &workspace, None).map_err(lsp_err)?;
                apply_edit_if_open(&self.client, &self.state, &uri, &content).await;
                if let Ok(Some((log_uri, log_content))) =
                    commands::regenerate_log_if_present_with_content(&jj, &workspace, None)
                {
                    apply_edit_if_open(&self.client, &self.state, &log_uri, &log_content).await;
                }
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.squash.toggle" => {
                let cp = commands::parse_cursor_arg(params.arguments.first())
                    .map_err(lsp_err)?
                    .ok_or_else(|| lsp_err("squash.toggle requires a cursor argument"))?;

                let (window, content) = {
                    let state = self.state.read().await;
                    let w = state
                        .open_squash_window
                        .clone()
                        .ok_or_else(|| lsp_err("no open squash window"))?;
                    let c = state
                        .documents
                        .get(&cp.uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(&cp.uri))
                        .ok_or_else(|| lsp_err(format!("document not found: {}", cp.uri)))?;
                    (w, c)
                };

                let section = cursor::squash_section_at_line(&content, cp.line)
                    .ok_or_else(|| lsp_err("cursor not in a SELECTED or REMAINING section"))?;

                let (squash_uri, new_content) = if let Some(file) =
                    cursor::squash_file_at_line(&content, cp.line)
                    && cursor::squash_hunk_at_line(&content, cp.line).is_none()
                {
                    // File-level toggle: use file-level squash (no --interactive needed).
                    commands::run_squash_toggle_file(&jj, &window, &file, section)
                        .map_err(lsp_err)?
                } else if let Some(hunk) = cursor::squash_hunk_at_line(&content, cp.line) {
                    // Hunk-level toggle: use --interactive --tool.
                    commands::run_squash_toggle_hunk(&jj, &workspace, &window, &hunk, section)
                        .map_err(lsp_err)?
                } else {
                    return Err(lsp_err("cursor is not on a file or hunk line"));
                };

                self.record_self_caused_op(&jj).await;
                apply_edit_if_open(&self.client, &self.state, &squash_uri, &new_content).await;
                self.state
                    .write()
                    .await
                    .documents
                    .insert(squash_uri.clone(), new_content);
                Ok(Some(serde_json::Value::String(squash_uri)))
            }
            "badjuju.squash.select_all" => {
                let window = self
                    .state
                    .read()
                    .await
                    .open_squash_window
                    .clone()
                    .ok_or_else(|| lsp_err("no open squash window"))?;
                let (squash_uri, new_content) =
                    commands::run_squash_select_all(&jj, &window).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                apply_edit_if_open(&self.client, &self.state, &squash_uri, &new_content).await;
                self.state
                    .write()
                    .await
                    .documents
                    .insert(squash_uri.clone(), new_content);
                Ok(Some(serde_json::Value::String(squash_uri)))
            }
            "badjuju.squash.select_none" => {
                let window = self
                    .state
                    .read()
                    .await
                    .open_squash_window
                    .clone()
                    .ok_or_else(|| lsp_err("no open squash window"))?;
                let (squash_uri, new_content) =
                    commands::run_squash_select_none(&jj, &window).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                apply_edit_if_open(&self.client, &self.state, &squash_uri, &new_content).await;
                self.state
                    .write()
                    .await
                    .documents
                    .insert(squash_uri.clone(), new_content);
                Ok(Some(serde_json::Value::String(squash_uri)))
            }
            "badjuju.squash.into" => {
                let arg = params
                    .arguments
                    .first()
                    .ok_or_else(|| lsp_err("badjuju.squash.into requires arguments"))?;
                let file = arg
                    .get("file")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| lsp_err("badjuju.squash.into: missing file"))?
                    .to_string();
                let parent_id = arg
                    .get("parentId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| lsp_err("badjuju.squash.into: missing parentId"))?
                    .to_string();
                let uri = commands::run_squash_into(&jj, &workspace, &file, &parent_id)
                    .map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.unsquash" => {
                let (file, revision) = resolve_file_scoped_args(
                    params.arguments.first(),
                    params.arguments.get(1),
                    &documents,
                )?;
                let uri =
                    commands::run_unsquash(&jj, &workspace, &file, &revision).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.undo" => {
                let uri = commands::run_undo(&jj, &workspace).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.abandon" => {
                let revision = commands::resolve_revision_arg(params.arguments.first(), |uri| {
                    documents
                        .get(uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(uri))
                })
                .map_err(lsp_err)?;
                let uri = commands::run_abandon(&jj, &workspace, &revision).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.edit" => {
                let revision = commands::resolve_revision_arg(params.arguments.first(), |uri| {
                    documents
                        .get(uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(uri))
                })
                .map_err(lsp_err)?;
                let uri = commands::run_edit(&jj, &workspace, &revision).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.fetch" => {
                let uri = commands::run_fetch(&jj, &workspace).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.push" => {
                let force_with_lease = params
                    .arguments
                    .first()
                    .and_then(|v| v.get("forceWithLease"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let uri = commands::run_push(&jj, &workspace, force_with_lease).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.rebase" => {
                let source = commands::resolve_revision_arg(params.arguments.first(), |uri| {
                    documents
                        .get(uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(uri))
                })
                .map_err(lsp_err)?;
                let dest = params
                    .arguments
                    .get(1)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let uri = commands::run_rebase(&jj, &workspace, &source, dest).map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            "badjuju.bookmark" => {
                let sub_action = params
                    .arguments
                    .first()
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let name = params
                    .arguments
                    .get(1)
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let revision = commands::resolve_revision_arg(params.arguments.get(2), |uri| {
                    documents
                        .get(uri)
                        .cloned()
                        .or_else(|| read_uri_from_disk(uri))
                })
                .map_err(lsp_err)?;
                let uri = commands::run_bookmark(&jj, &workspace, sub_action, name, &revision)
                    .map_err(lsp_err)?;
                self.record_self_caused_op(&jj).await;
                self.refresh_open_diffs(&jj, &workspace).await;
                Ok(Some(serde_json::Value::String(uri)))
            }
            _ => Err(Error::method_not_found()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_list_is_nonempty() {
        assert!(!COMMANDS.is_empty());
    }

    #[test]
    fn parse_diff_uri_three_slash_change() {
        assert_eq!(
            parse_diff_uri("badjuju-diff:///change/abc123").unwrap(),
            DiffUriKind::Change("abc123")
        );
    }

    #[test]
    fn parse_diff_uri_one_slash_change() {
        // VS Code normalizes empty-authority URIs to `scheme:/path`.
        assert_eq!(
            parse_diff_uri("badjuju-diff:/change/abc123").unwrap(),
            DiffUriKind::Change("abc123")
        );
    }

    #[test]
    fn parse_diff_uri_three_slash_commit() {
        assert_eq!(
            parse_diff_uri("badjuju-diff:///commit/deadbeef").unwrap(),
            DiffUriKind::Commit("deadbeef")
        );
    }

    #[test]
    fn parse_diff_uri_one_slash_commit() {
        assert_eq!(
            parse_diff_uri("badjuju-diff:/commit/deadbeef").unwrap(),
            DiffUriKind::Commit("deadbeef")
        );
    }

    #[test]
    fn parse_diff_uri_wrong_scheme_errors() {
        assert!(parse_diff_uri("file:///foo").is_err());
        assert!(parse_diff_uri("badjuju-status:///x").is_err());
    }

    #[test]
    fn parse_diff_uri_empty_id_errors() {
        assert!(parse_diff_uri("badjuju-diff:///change/").is_err());
        assert!(parse_diff_uri("badjuju-diff:///commit/").is_err());
        assert!(parse_diff_uri("badjuju-diff:/change/").is_err());
    }

    #[test]
    fn parse_diff_uri_unknown_path_errors() {
        assert!(parse_diff_uri("badjuju-diff:///foobar/x").is_err());
    }

    #[test]
    fn commands_include_expected() {
        assert!(COMMANDS.contains(&"badjuju.status"));
        assert!(COMMANDS.contains(&"badjuju.log"));
        assert!(COMMANDS.contains(&"badjuju.describe"));
        assert!(COMMANDS.contains(&"badjuju.diff"));
        assert!(COMMANDS.contains(&"badjuju.diff.commit"));
        assert!(COMMANDS.contains(&"badjuju.new"));
        assert!(COMMANDS.contains(&"badjuju.next"));
        assert!(COMMANDS.contains(&"badjuju.prev"));
        assert!(COMMANDS.contains(&"badjuju.refresh"));
        assert!(COMMANDS.contains(&"badjuju.squash"));
        assert!(COMMANDS.contains(&"badjuju.unsquash"));
        assert!(COMMANDS.contains(&"badjuju.undo"));
        assert!(COMMANDS.contains(&"badjuju.abandon"));
        assert!(COMMANDS.contains(&"badjuju.keymap"));
        assert!(COMMANDS.contains(&"badjuju.help"));
        assert!(COMMANDS.contains(&"badjuju.version"));
        assert!(COMMANDS.contains(&"badjuju.edit"));
        assert!(COMMANDS.contains(&"badjuju.fetch"));
        assert!(COMMANDS.contains(&"badjuju.push"));
        assert!(COMMANDS.contains(&"badjuju.rebase"));
    }

    #[test]
    fn take_if_self_caused_returns_true_exactly_once() {
        let mut state = State::default();
        state.record_self_caused_op("abc123".to_string());
        assert!(
            state.take_if_self_caused("abc123"),
            "first call should return true"
        );
        assert!(
            !state.take_if_self_caused("abc123"),
            "second call should return false"
        );
    }

    #[test]
    fn take_if_self_caused_unknown_op_returns_false() {
        let mut state = State::default();
        assert!(!state.take_if_self_caused("not-recorded"));
    }

    #[test]
    fn record_self_caused_op_prunes_stale_entries() {
        let mut state = State::default();
        // Manually insert an entry with an Instant 11 seconds in the past.
        let old_instant = Instant::now() - Duration::from_secs(11);
        state
            .self_caused_ops
            .insert("stale".to_string(), old_instant);
        // Recording a new op triggers pruning of stale entries.
        state.record_self_caused_op("fresh".to_string());
        assert!(
            !state.self_caused_ops.contains_key("stale"),
            "stale entry should be pruned"
        );
        assert!(
            state.self_caused_ops.contains_key("fresh"),
            "fresh entry should survive"
        );
    }
}
