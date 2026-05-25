use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::Url;

use crate::cursor::{self, BufferKind};
use crate::jj::{Jj, JjError};
use crate::keymap::{self, KeymapProfile};

const STATUS_REVSET: &str = "ancestors(reachable(@, mutable()), 2)";

/// Default revset for the log window when the client passes no explicit
/// revset. Matches STATUS_REVSET so an unconfigured log open mirrors the
/// stack view shown in status.jujutsu.
const DEFAULT_LOG_REVSET: &str = STATUS_REVSET;

/// In-buffer COMMAND REFERENCE text for each generated buffer type.
///
/// Defaults are rendered from the active `KeymapProfile`. Clients may supply
/// override text via `initializationOptions.commandReference` as an escape
/// hatch when their actual keybindings differ from the profile's defaults.
#[derive(Debug, Clone)]
pub struct CommandReference {
    status: String,
    log: String,
    diff: String,
}

impl Default for CommandReference {
    fn default() -> Self {
        Self::from_profile(&KeymapProfile::Magit)
    }
}

impl CommandReference {
    /// Render all three buffers' reference text from the given profile.
    pub fn from_profile(profile: &KeymapProfile) -> Self {
        Self {
            status: keymap::render_command_reference(profile, "status"),
            log: keymap::render_command_reference(profile, "log"),
            diff: keymap::render_command_reference(profile, "diff"),
        }
    }

    /// Build from a profile with optional per-buffer client overrides.
    ///
    /// `None` for a field means "use the profile default"; a `Some` value
    /// replaces the rendered text entirely (the escape-hatch path used by
    /// clients whose keybindings differ from any built-in profile).
    pub fn new(status: Option<String>, log: Option<String>, diff: Option<String>) -> Self {
        Self::with_profile(&KeymapProfile::Magit, status, log, diff)
    }

    pub fn with_profile(
        profile: &KeymapProfile,
        status: Option<String>,
        log: Option<String>,
        diff: Option<String>,
    ) -> Self {
        let base = Self::from_profile(profile);
        Self {
            status: status.unwrap_or(base.status),
            log: log.unwrap_or(base.log),
            diff: diff.unwrap_or(base.diff),
        }
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn log(&self) -> &str {
        &self.log
    }

    pub fn diff(&self) -> &str {
        &self.diff
    }
}

/// Pre-defined revset shortcuts shown in the log.jujutsu header.
/// Each entry is (label, revset). The label is also used to align columns.
const LOG_SHORTCUTS: &[(&str, &str)] = &[
    ("Mutable", "ancestors(reachable(@, mutable()), 2)"),
    ("Stack", "(immutable_heads()..@)::"),
];

/// Render the shortcut list as `JJ:` comment lines for the log.jujutsu header.
fn render_log_shortcuts() -> String {
    let label_width = LOG_SHORTCUTS
        .iter()
        .map(|(label, _)| label.len())
        .max()
        .unwrap_or(0);
    LOG_SHORTCUTS
        .iter()
        .map(|(label, revset)| {
            let padding = " ".repeat(label_width - label.len() + 2);
            format!("JJ: {label}:{padding}{revset}")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Returns the `<workspace>/.jj/badjuju/` directory, creating it if needed.
fn badjuju_dir(workspace: &Path) -> std::io::Result<PathBuf> {
    let dir = workspace.join(".jj").join("badjuju");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Render an absolute filesystem path as a `file://` URI the client can open.
///
/// Uses [`Url::from_file_path`] so the result is well-formed on every platform.
/// On Unix the output is `file:///abs/path`; on Windows it's
/// `file:///C:/Users/…` — note the leading `/` before the drive letter. A naive
/// `format!("file://{}", path.display())` produces `file://C:\Users\…` on
/// Windows, which VS Code rejects with "Unable to read file '\'" (bad-juju-z5j).
fn file_uri(path: &Path) -> String {
    Url::from_file_path(path)
        .expect("badjuju output paths are always absolute")
        .to_string()
}

/// Inverse of [`file_uri`]: convert a `file://` URI back to a filesystem path.
///
/// Returns `None` for non-`file:` URIs or paths the URL crate can't decode
/// (e.g. relative paths, missing Windows drive prefixes). Used by `run_refresh`
/// to read the file the client just told us to regenerate.
pub(crate) fn path_from_uri(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

/// Run `badjuju.status`: write status.jujutsu (preserving any current STATS toggle) and return its URI.
pub fn run_status(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    write_status(jj, workspace, None, stat)
}

/// Toggle the STATS state and re-render.
pub fn run_toggle_stat(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    let next = !read_current_stat(workspace);
    write_status(jj, workspace, None, next)
}

/// Read the persisted STATS state from the sidecar file. Defaults to `false`.
fn read_current_stat(workspace: &Path) -> bool {
    let Ok(dir) = badjuju_dir(workspace) else {
        return false;
    };
    match std::fs::read_to_string(dir.join("stats")) {
        Ok(s) => s.trim() == "on",
        Err(_) => false,
    }
}

fn write_stat_state(workspace: &Path, stat: bool) -> std::io::Result<()> {
    let dir = badjuju_dir(workspace)?;
    std::fs::write(dir.join("stats"), if stat { "on\n" } else { "off\n" })
}

/// Write status.jujutsu, optionally prepending a MESSAGE: block. Returns the URI.
fn write_status(
    jj: &Jj,
    workspace: &Path,
    message: Option<&str>,
    stat: bool,
) -> Result<String, CommandError> {
    let status = jj.status()?;
    let stack = jj.log_with_stat(STATUS_REVSET, stat)?;

    let prelude = match message {
        Some(m) => format!("MESSAGE: {}\n\n", m.trim()),
        None => String::new(),
    };

    let content = format!(
        "{}STATUS:\n\n{}\n\nSTACK: {}\n\n{}\n\n{}",
        prelude,
        status.trim_end(),
        STATUS_REVSET,
        stack.trim_end(),
        jj.command_reference().status(),
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("status.jujutsu");
    std::fs::write(&path, content)?;
    write_stat_state(workspace, stat)?;
    Ok(file_uri(&path))
}

/// Normalize a revision argument from the client. Empty string falls back to `@`.
fn revision_or_at(revision: &str) -> &str {
    if revision.is_empty() { "@" } else { revision }
}

/// Run `badjuju.squash`: move `file` from `revision` into `revision`'s parent (`jj squash -r REV FILE`).
/// If `revision` has anything other than exactly one parent, no action is taken and the
/// status buffer reports the error.
pub fn run_squash(
    jj: &Jj,
    workspace: &Path,
    file: &str,
    revision: &str,
) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    let rev = revision_or_at(revision);
    if file.is_empty() {
        return write_status(jj, workspace, Some("squash: no file selected"), stat);
    }
    let parents = jj.change_ids(&format!("parents({rev})"))?;
    if parents.len() != 1 {
        return write_status(
            jj,
            workspace,
            Some(&format!(
                "squash {file} from {rev}: revision has {} parents (need exactly 1)",
                parents.len()
            )),
            stat,
        );
    }
    match jj.squash_file_into_parent(rev, file) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("squash {file} from {rev} failed: {e}")),
            stat,
        ),
    }
}

/// Run `badjuju.unsquash`: move `file` from `revision` into `revision`'s immediate child
/// (`jj squash --from REV --into CHILD FILE`). Errors if 0 or >1 children.
pub fn run_unsquash(
    jj: &Jj,
    workspace: &Path,
    file: &str,
    revision: &str,
) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    let rev = revision_or_at(revision);
    if file.is_empty() {
        return write_status(jj, workspace, Some("unsquash: no file selected"), stat);
    }
    let children = jj.change_ids(&format!("({rev})+"))?;
    if children.len() != 1 {
        return write_status(
            jj,
            workspace,
            Some(&format!(
                "unsquash {file} from {rev}: revision has {} children (need exactly 1)",
                children.len()
            )),
            stat,
        );
    }
    match jj.squash_file_into(rev, &children[0], file) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("unsquash {file} from {rev} failed: {e}")),
            stat,
        ),
    }
}

/// Run `badjuju.log`: write log.jujutsu and return its URI.
pub fn run_log(jj: &Jj, workspace: &Path, revset: &str) -> Result<String, CommandError> {
    let revset = if revset.is_empty() {
        DEFAULT_LOG_REVSET
    } else {
        revset
    };
    let output = jj.log(revset)?;

    let content = format!(
        "REVSET: {}\n{}\n\nOUTPUT:\n\n{}\n\n{}",
        revset,
        render_log_shortcuts(),
        output.trim_end(),
        jj.command_reference().log(),
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("log.jujutsu");
    std::fs::write(&path, content)?;
    Ok(file_uri(&path))
}

/// Run `badjuju.diff`: write diff.jujutsu showing `jj diff -r REV` for the
/// given revision (defaults to `@` when empty). Embeds a `REVISION:` header
/// so refresh can re-run against the same commit.
pub fn run_diff(jj: &Jj, workspace: &Path, revision: &str) -> Result<String, CommandError> {
    let rev = revision_or_at(revision);
    let output = jj.diff(rev)?;

    let content = format!(
        "REVISION: {}\n\nDIFF:\n\n{}\n\n{}",
        rev,
        output.trim_end(),
        jj.command_reference().diff(),
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("diff.jujutsu");
    std::fs::write(&path, content)?;
    Ok(file_uri(&path))
}

/// Extract the revision from the `REVISION:` header of diff.jujutsu. Used by
/// `run_refresh` so refreshing a diff buffer re-runs against the same commit
/// rather than falling back to status.
pub fn parse_diff_revision(content: &str) -> Option<String> {
    let first = content.lines().next()?;
    let rest = first.strip_prefix("REVISION:")?.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

/// Run `badjuju.describe`: write describe.jujutsu with the current description
/// of `revision` (defaults to `@` when empty). Embeds a `JJ: revision: <rev>`
/// header line so `on_describe_save` can route the saved description back to
/// the same commit.
pub fn run_describe(jj: &Jj, workspace: &Path, revision: &str) -> Result<String, CommandError> {
    let rev = revision_or_at(revision);
    let current_desc = jj.describe_get(rev)?;
    let desc = if current_desc.trim().is_empty() {
        String::new()
    } else {
        current_desc.trim_end().to_string()
    };

    let content = format!(
        "{}\n\
         \n\
         JJ: ------------------------ >8 ------------------------\n\
         JJ: Do not modify or remove the separator line above.\n\
         JJ: Edit the description above and save this file.\n\
         JJ: revision: {}\n\
         JJ: Lines starting with 'JJ:' will be removed.\n",
        desc, rev,
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("describe.jujutsu");
    std::fs::write(&path, content)?;
    Ok(file_uri(&path))
}

/// Run `badjuju.refresh`: regenerate the file identified by `uri`.
/// For status.jujutsu → regenerate status. For log.jujutsu → re-run log with current REVSET header.
/// Falls back to status when the URI doesn't decode to a known badjuju buffer.
pub fn run_refresh(jj: &Jj, workspace: &Path, uri: &str) -> Result<String, CommandError> {
    let Some(path) = path_from_uri(uri) else {
        return run_status(jj, workspace);
    };
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    match filename {
        "log.jujutsu" => {
            let content = std::fs::read_to_string(&path)?;
            let revset = parse_log_revset(&content).unwrap_or_else(|| "@".to_string());
            run_log(jj, workspace, &revset)
        }
        "diff.jujutsu" => {
            let content = std::fs::read_to_string(&path)?;
            let revision = parse_diff_revision(&content).unwrap_or_else(|| "@".to_string());
            run_diff(jj, workspace, &revision)
        }
        _ => run_status(jj, workspace),
    }
}

/// Run `badjuju.new`: create a new change and regenerate status.jujutsu.
///
/// When `parent` is empty, the new change is a child of `@` (default `jj new`).
/// When non-empty, the new change is created as a child of that revision
/// (`jj new <REV>`), so e.g. the commit under the user's cursor becomes the
/// parent. Returns the status URI.
pub fn run_new(jj: &Jj, workspace: &Path, parent: &str) -> Result<String, CommandError> {
    jj.new_change(parent)?;
    run_status(jj, workspace)
}

/// Run `badjuju.next`: move the working copy to a child revision (`jj next`),
/// optionally with `--edit`. On failure, surface the error as a MESSAGE prelude
/// in the status buffer (typical when @ has no descendants).
pub fn run_next(jj: &Jj, workspace: &Path, edit: bool) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    match jj.next_change(edit) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => {
            let label = if edit { "next --edit" } else { "next" };
            write_status(jj, workspace, Some(&format!("{label} failed: {e}")), stat)
        }
    }
}

/// Run `badjuju.prev`: move the working copy to an ancestor revision (`jj prev`),
/// optionally with `--edit`. On failure, surface the error as a MESSAGE prelude
/// in the status buffer.
pub fn run_prev(jj: &Jj, workspace: &Path, edit: bool) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    match jj.prev_change(edit) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => {
            let label = if edit { "prev --edit" } else { "prev" };
            write_status(jj, workspace, Some(&format!("{label} failed: {e}")), stat)
        }
    }
}

/// Run `badjuju.undo`: revert the last operation with `jj undo`, then refresh status.
/// Surfaces failures as a MESSAGE: prelude in the status buffer.
pub fn run_undo(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    match jj.undo() {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(jj, workspace, Some(&format!("undo failed: {e}")), stat),
    }
}

/// Run `badjuju.rebase`: rebase `source` onto `dest` (`jj rebase -s SRC -d DEST`),
/// then refresh status and log. Surfaces failures as a MESSAGE prelude.
/// `source` defaults to `@` when empty; `dest` must be non-empty.
pub fn run_rebase(
    jj: &Jj,
    workspace: &Path,
    source: &str,
    dest: &str,
) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    if dest.is_empty() {
        return write_status(
            jj,
            workspace,
            Some("rebase: destination revision required"),
            stat,
        );
    }
    let src = revision_or_at(source);
    match jj.rebase(src, dest) {
        Ok(()) => {
            regenerate_log_if_present(jj, workspace)?;
            run_status(jj, workspace)
        }
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("rebase {src} to {dest} failed: {e}")),
            stat,
        ),
    }
}

/// Run `badjuju.push`: run `jj git push`, then refresh status.
/// jj push already has force-with-lease semantics by default; the
/// `force_with_lease` parameter is accepted for API consistency but has no
/// effect on the underlying command.
pub fn run_push(
    jj: &Jj,
    workspace: &Path,
    _force_with_lease: bool,
) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    match jj.git_push() {
        Ok(_) => run_status(jj, workspace),
        Err(e) => write_status(jj, workspace, Some(&format!("push failed: {e}")), stat),
    }
}

/// Run `badjuju.fetch`: run `jj git fetch`, then refresh status.
/// Surfaces failures as a MESSAGE prelude.
pub fn run_fetch(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    match jj.git_fetch() {
        Ok(_) => run_status(jj, workspace),
        Err(e) => write_status(jj, workspace, Some(&format!("fetch failed: {e}")), stat),
    }
}

/// Run `badjuju.edit`: move @ to `revision` (`jj edit REV`), then refresh status
/// and log (if log file exists). Surfaces failures as a MESSAGE prelude.
pub fn run_edit(jj: &Jj, workspace: &Path, revision: &str) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    let rev = revision_or_at(revision);
    match jj.edit(rev) {
        Ok(()) => {
            regenerate_log_if_present(jj, workspace)?;
            run_status(jj, workspace)
        }
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("edit {rev} failed: {e}")),
            stat,
        ),
    }
}

/// Run `badjuju.abandon`: abandon `revision` (defaults to `@`) and refresh status.
/// Surfaces failures as a MESSAGE: prelude in the status buffer.
pub fn run_abandon(jj: &Jj, workspace: &Path, revision: &str) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    let rev = revision_or_at(revision);
    match jj.abandon(rev) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("abandon {rev} failed: {e}")),
            stat,
        ),
    }
}

/// Dispatch a bookmark sub-action, then refresh status + log.
/// `sub_action` must be one of "create", "move", "delete", "track", "forget".
/// `name` is required for all sub-actions.
/// `revision` is used by create and move (defaults to @); ignored by others.
/// Surfaces failures as a MESSAGE prelude.
pub fn run_bookmark(
    jj: &Jj,
    workspace: &Path,
    sub_action: &str,
    name: &str,
    revision: &str,
) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    if name.is_empty() {
        return write_status(jj, workspace, Some("bookmark: name is required"), stat);
    }
    let result = match sub_action {
        "create" => jj.bookmark_create(name, revision),
        "move" => jj.bookmark_move(name, revision),
        "delete" => jj.bookmark_delete(name),
        "track" => jj.bookmark_track(name),
        "forget" => jj.bookmark_forget(name),
        other => {
            return write_status(
                jj,
                workspace,
                Some(&format!("bookmark: unknown sub-action '{other}'")),
                stat,
            );
        }
    };
    match result {
        Ok(()) => {
            regenerate_log_if_present(jj, workspace)?;
            run_status(jj, workspace)
        }
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("bookmark {sub_action} failed: {e}")),
            stat,
        ),
    }
}

/// Strip JJ: comment lines and the separator from describe.jujutsu content.
/// Returns the trimmed description, or `None` if nothing remains.
pub fn parse_describe_content(content: &str) -> Option<String> {
    let stripped: Vec<&str> = content
        .lines()
        .take_while(|line| !line.starts_with("JJ:"))
        .collect();
    let trimmed = stripped.join("\n").trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Extract the revset from the `REVSET:` header block of log.jujutsu.
///
/// The block begins on the line starting with `REVSET: ` and continues across
/// subsequent lines until a blank line or an `OUTPUT:` section header is
/// reached. `JJ:` comment lines inside the block are skipped, so the rendered
/// shortcut comments don't bleed into the revset. Continuation lines preserve
/// the user's formatting (trimmed of trailing whitespace) and are joined with
/// newlines — jj treats newlines as ordinary whitespace inside a revset.
pub fn parse_log_revset(content: &str) -> Option<String> {
    let mut lines = content.lines();
    let first = lines.next()?.strip_prefix("REVSET:")?;
    let mut parts: Vec<String> = Vec::new();
    let first = first.strip_prefix(' ').unwrap_or(first).trim_end();
    if !first.is_empty() {
        parts.push(first.to_string());
    }
    for line in lines {
        if line.is_empty() || line.starts_with("OUTPUT:") {
            break;
        }
        if line.starts_with("JJ:") {
            continue;
        }
        let trimmed_end = line.trim_end();
        if !trimmed_end.is_empty() {
            parts.push(trimmed_end.to_string());
        }
    }
    let joined = parts.join("\n");
    if joined.trim().is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// Extract the embedded `JJ: revision: <rev>` header from describe.jujutsu
/// content. Returns `None` when no header is present or the value is empty,
/// in which case callers should fall back to `@`.
pub fn parse_describe_revision(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("JJ: revision:") {
            let rev = rest.trim();
            if !rev.is_empty() {
                return Some(rev.to_string());
            }
        }
    }
    None
}

/// On describe.jujutsu save: apply stripped description via `jj describe -r REV`,
/// then regenerate status.jujutsu and, when an existing log.jujutsu is present,
/// regenerate it too so its rendered descriptions stay in sync. The revision is
/// taken from the embedded `JJ: revision:` header so the description routes
/// back to the same commit that was opened, even when @ has since moved.
pub fn on_describe_save(jj: &Jj, workspace: &Path, content: &str) -> Result<(), CommandError> {
    if let Some(desc) = parse_describe_content(content) {
        let rev = parse_describe_revision(content).unwrap_or_else(|| "@".to_string());
        jj.describe_set(&rev, &desc)?;
        run_status(jj, workspace)?;
        regenerate_log_if_present(jj, workspace)?;
    }
    Ok(())
}

/// Regenerate log.jujutsu when it already exists on disk (i.e. the log window
/// has been opened in this workspace). Preserves the persisted REVSET header
/// so the same query is re-run. No-op when the file is absent.
fn regenerate_log_if_present(jj: &Jj, workspace: &Path) -> Result<(), CommandError> {
    let log_path = workspace.join(".jj").join("badjuju").join("log.jujutsu");
    if !log_path.exists() {
        return Ok(());
    }
    let content = std::fs::read_to_string(&log_path)?;
    let revset = parse_log_revset(&content).unwrap_or_else(|| DEFAULT_LOG_REVSET.to_string());
    run_log(jj, workspace, &revset)?;
    Ok(())
}

/// On log.jujutsu save: re-parse the REVSET: header and regenerate the file.
pub fn on_log_save(jj: &Jj, workspace: &Path, content: &str) -> Result<String, CommandError> {
    let revset = parse_log_revset(content).unwrap_or_else(|| "@".to_string());
    run_log(jj, workspace, &revset)
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("jj error: {0}")]
    Jj(#[from] JjError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// --- Cursor-form argument resolution ----------------------------------------

/// Error returned when a cursor-form command argument cannot be resolved.
#[derive(Debug, thiserror::Error)]
pub enum CursorResolveError {
    #[error("invalid cursor argument shape; expected {{cursor:{{uri,line}}}}")]
    InvalidArg,
    #[error("unsupported buffer URI for cursor argument: {0}")]
    UnsupportedBuffer(String),
    #[error("document content not available for {0}")]
    DocNotFound(String),
    #[error("no revision at cursor position")]
    NoRevisionAtCursor,
    #[error("no file at cursor position")]
    NoFileAtCursor,
    #[error("no log shortcut at cursor position")]
    NoShortcutAtCursor,
}

/// Parsed `{cursor: {uri, line}}` command argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorPos {
    pub uri: String,
    pub line: usize,
}

/// Extract a `{cursor: {uri, line}}` object from a single argument value.
///
/// Returns `Ok(None)` for null / missing values so callers can apply a default
/// (typically `@`). Strings return `Ok(None)` too — callers that accept the
/// legacy literal-string form (`resolve_revision_arg`,
/// `resolve_file_and_revision_arg`, `resolve_log_shortcut_arg`) check for
/// strings themselves before invoking this. Returns `Err(InvalidArg)` for
/// objects that look cursor-shaped but are malformed.
pub fn parse_cursor_arg(
    arg: Option<&serde_json::Value>,
) -> std::result::Result<Option<CursorPos>, CursorResolveError> {
    let Some(v) = arg else {
        return Ok(None);
    };
    if v.is_null() || v.is_string() {
        return Ok(None);
    }
    let cursor = v.get("cursor").ok_or(CursorResolveError::InvalidArg)?;
    let uri = cursor
        .get("uri")
        .and_then(|x| x.as_str())
        .ok_or(CursorResolveError::InvalidArg)?
        .to_string();
    let line = cursor
        .get("line")
        .and_then(|x| x.as_u64())
        .ok_or(CursorResolveError::InvalidArg)? as usize;
    Ok(Some(CursorPos { uri, line }))
}

/// Resolve a revision argument. Accepts either a literal revision string
/// (e.g. `"@-"` from CLI user commands or pre-resolved code actions), a
/// `{cursor:{uri,line}}` object, or a missing / null arg (defaults to empty
/// → `@` downstream).
pub fn resolve_revision_arg<F>(
    arg: Option<&serde_json::Value>,
    doc_lookup: F,
) -> std::result::Result<String, CursorResolveError>
where
    F: FnOnce(&str) -> Option<String>,
{
    if let Some(s) = arg.and_then(|v| v.as_str()) {
        return Ok(s.to_string());
    }
    let Some(cp) = parse_cursor_arg(arg)? else {
        return Ok(String::new());
    };
    let kind = BufferKind::from_uri(&cp.uri)
        .ok_or_else(|| CursorResolveError::UnsupportedBuffer(cp.uri.clone()))?;
    let content =
        doc_lookup(&cp.uri).ok_or_else(|| CursorResolveError::DocNotFound(cp.uri.clone()))?;
    cursor::revision_at_line(&content, cp.line, kind).ok_or(CursorResolveError::NoRevisionAtCursor)
}

/// Resolve both file and revision for file-scoped commands like squash and
/// unsquash. Accepts either the legacy `[file_str, revision_str]` form (Neovim
/// CLI: `:JJSquash <file> @-`) or a single `{cursor:{uri,line}}` arg passed
/// as `file_arg` (code actions). Missing args error with `InvalidArg`.
pub fn resolve_file_and_revision_arg<F>(
    file_arg: Option<&serde_json::Value>,
    rev_arg: Option<&serde_json::Value>,
    doc_lookup: F,
) -> std::result::Result<(String, String), CursorResolveError>
where
    F: FnOnce(&str) -> Option<String>,
{
    if let (Some(f), Some(r)) = (
        file_arg.and_then(|v| v.as_str()),
        rev_arg.and_then(|v| v.as_str()),
    ) {
        return Ok((f.to_string(), r.to_string()));
    }
    let cp = parse_cursor_arg(file_arg)?.ok_or(CursorResolveError::InvalidArg)?;
    let kind = BufferKind::from_uri(&cp.uri)
        .ok_or_else(|| CursorResolveError::UnsupportedBuffer(cp.uri.clone()))?;
    let content =
        doc_lookup(&cp.uri).ok_or_else(|| CursorResolveError::DocNotFound(cp.uri.clone()))?;
    let file = cursor::file_at_line(&content, cp.line).ok_or(CursorResolveError::NoFileAtCursor)?;
    let revision = cursor::revision_at_line(&content, cp.line, kind)
        .ok_or(CursorResolveError::NoRevisionAtCursor)?;
    Ok((file, revision))
}

/// Resolve a log-shortcut revset from a cursor-form arg pointing at a
/// `JJ: <Label>: <revset>` line in `log.jujutsu`.
///
/// Returns `Ok(None)` for non-cursor args (the `badjuju.log` adapter falls
/// through to literal-revset handling). String revsets are still valid for
/// `badjuju.log`; T19 only removed the legacy form from revision-scoped
/// commands.
pub fn resolve_log_shortcut_arg<F>(
    arg: Option<&serde_json::Value>,
    doc_lookup: F,
) -> std::result::Result<Option<String>, CursorResolveError>
where
    F: FnOnce(&str) -> Option<String>,
{
    let Some(cp) = parse_cursor_arg(arg)? else {
        return Ok(None);
    };
    if BufferKind::from_uri(&cp.uri) != Some(BufferKind::Log) {
        return Err(CursorResolveError::UnsupportedBuffer(cp.uri));
    }
    let content =
        doc_lookup(&cp.uri).ok_or_else(|| CursorResolveError::DocNotFound(cp.uri.clone()))?;
    let shortcut = cursor::log_shortcut_at_line(&content, cp.line)
        .ok_or(CursorResolveError::NoShortcutAtCursor)?;
    Ok(Some(shortcut.revset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    fn init_repo(dir: &Path) -> Jj {
        Command::new("jj")
            .args(["git", "init"])
            .current_dir(dir)
            .output()
            .expect("jj git init failed");
        Jj::new("jj", dir)
    }

    #[test]
    fn run_status_writes_file_and_returns_uri() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        assert!(uri.starts_with("file://"));
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("STATUS:"));
        assert!(content.contains("STACK:"));
        assert!(content.contains("COMMAND REFERENCE:"));
    }

    #[test]
    fn run_status_command_reference_matches_keybindings() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        // Every key in the magit status profile must appear at the start of a line.
        for key in [
            "n", "L", "r", "e", "d", "D", "s", "U", "a", "f", "p", "P", "u", "=", "R", "q", "?",
        ] {
            assert!(
                content.lines().any(|l| l.starts_with(key)),
                "missing key `{key}` in status command reference:\n{content}"
            );
        }
        // 'r' IS now in status — bound to badjuju.rebase.
    }

    #[test]
    fn command_reference_defaults_render_from_magit_profile() {
        use crate::keymap::{KeymapProfile, render_command_reference};
        let default = CommandReference::default();
        assert_eq!(
            default.status(),
            render_command_reference(&KeymapProfile::Magit, "status")
        );
        assert_eq!(
            default.log(),
            render_command_reference(&KeymapProfile::Magit, "log")
        );
        assert_eq!(
            default.diff(),
            render_command_reference(&KeymapProfile::Magit, "diff")
        );
    }

    #[test]
    fn command_reference_override_passes_through_each_buffer() {
        let dir = tempdir().unwrap();
        let reference = CommandReference::new(
            Some("CUSTOM STATUS REF".to_string()),
            Some("CUSTOM LOG REF".to_string()),
            Some("CUSTOM DIFF REF".to_string()),
        );
        let jj = Jj::with_binary_or_default(None, dir.path()).with_command_reference(reference);
        std::process::Command::new("jj")
            .args(["git", "init"])
            .current_dir(dir.path())
            .output()
            .expect("jj git init failed");

        let status_uri = run_status(&jj, dir.path()).unwrap();
        let status_content =
            std::fs::read_to_string(status_uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            status_content.contains("CUSTOM STATUS REF"),
            "expected status override in:\n{status_content}"
        );
        assert!(
            !status_content
                .lines()
                .any(|l| l.starts_with("n") && l.contains("new change")),
            "default reference text should not leak through when overridden:\n{status_content}"
        );

        let log_uri = run_log(&jj, dir.path(), "@").unwrap();
        let log_content =
            std::fs::read_to_string(log_uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            log_content.contains("CUSTOM LOG REF"),
            "expected log override in:\n{log_content}"
        );

        let diff_uri = run_diff(&jj, dir.path(), "@").unwrap();
        let diff_content =
            std::fs::read_to_string(diff_uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            diff_content.contains("CUSTOM DIFF REF"),
            "expected diff override in:\n{diff_content}"
        );
    }

    #[test]
    fn command_reference_partial_override_falls_back_per_field() {
        let dir = tempdir().unwrap();
        // Only override the log reference; status and diff should use defaults.
        let reference = CommandReference::new(None, Some("LOG ONLY OVERRIDE".to_string()), None);
        let jj = Jj::with_binary_or_default(None, dir.path()).with_command_reference(reference);
        std::process::Command::new("jj")
            .args(["git", "init"])
            .current_dir(dir.path())
            .output()
            .expect("jj git init failed");

        let status_content = std::fs::read_to_string(
            run_status(&jj, dir.path())
                .unwrap()
                .strip_prefix("file://")
                .unwrap(),
        )
        .unwrap();
        assert!(
            status_content.lines().any(|l| l.starts_with("n")),
            "status reference should still be the default (n = new change); got:\n{status_content}"
        );

        let log_content = std::fs::read_to_string(
            run_log(&jj, dir.path(), "@")
                .unwrap()
                .strip_prefix("file://")
                .unwrap(),
        )
        .unwrap();
        assert!(
            log_content.contains("LOG ONLY OVERRIDE"),
            "log reference should be overridden; got:\n{log_content}"
        );
    }

    #[test]
    fn run_log_writes_file_with_revset_header() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_log(&jj, dir.path(), "@").expect("run_log failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.starts_with("REVSET: @"));
    }

    #[test]
    fn run_log_includes_output_heading_and_command_reference() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_log(&jj, dir.path(), "@").expect("run_log failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(
            content.contains("OUTPUT:"),
            "missing OUTPUT heading:\n{content}"
        );
        assert!(
            content.contains("COMMAND REFERENCE:"),
            "missing command reference:\n{content}"
        );
        assert!(
            content.contains("Edit REVSET above"),
            "missing revset edit hint:\n{content}"
        );
        assert!(
            content
                .lines()
                .any(|l| l.starts_with("a") && l.contains("abandon")),
            "missing abandon hint:\n{content}"
        );
    }

    #[test]
    fn run_log_uses_provided_revset() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let revset = "@ | @-";
        let uri = run_log(&jj, dir.path(), revset).expect("run_log failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.starts_with(&format!("REVSET: {revset}")));
    }

    #[test]
    fn run_log_empty_revset_defaults_to_status_stack() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_log(&jj, dir.path(), "").expect("run_log failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(
            content.starts_with("REVSET: ancestors(reachable(@, mutable()), 2)"),
            "empty revset should default to the depth-2 mutable revset:\n{content}"
        );
    }

    #[test]
    fn run_log_renders_revset_shortcuts_after_header() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_log(&jj, dir.path(), "@").expect("run_log failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();

        assert!(
            content.contains("JJ: Mutable:"),
            "missing Mutable shortcut:\n{content}"
        );
        assert!(
            content.contains("ancestors(reachable(@, mutable()), 2)"),
            "missing Mutable revset:\n{content}"
        );
        assert!(
            content.contains("JJ: Stack:"),
            "missing Stack shortcut:\n{content}"
        );
        assert!(
            content.contains("(immutable_heads()..@)::"),
            "missing Stack revset:\n{content}"
        );

        let revset_line_idx = content
            .lines()
            .position(|l| l.starts_with("REVSET:"))
            .expect("REVSET line not found");
        let mutable_line_idx = content
            .lines()
            .position(|l| l.starts_with("JJ: Mutable:"))
            .expect("Mutable shortcut line not found");
        assert!(
            mutable_line_idx > revset_line_idx,
            "Mutable shortcut should appear after REVSET line"
        );
    }

    #[test]
    fn run_log_shortcut_lines_use_jj_comment_prefix() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_log(&jj, dir.path(), "@").expect("run_log failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();

        for (label, _) in LOG_SHORTCUTS {
            let prefix = format!("JJ: {label}:");
            let found = content.lines().any(|line| line.starts_with(&prefix));
            assert!(found, "no `JJ: {label}:` line found in:\n{content}");
        }
    }

    #[test]
    fn on_log_save_ignores_shortcut_comment_lines() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Simulate a saved log.jujutsu that still contains the shortcut comment lines.
        let content = format!(
            "REVSET: @\n{}\n\nOUTPUT:\n\nstale output",
            render_log_shortcuts()
        );
        let uri = on_log_save(&jj, dir.path(), &content).expect("on_log_save failed");
        let new_content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        // REVSET should still be `@` — the JJ: lines must not have hijacked the header.
        assert!(new_content.starts_with("REVSET: @"));
    }

    #[test]
    fn run_describe_writes_file_with_separator() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_describe(&jj, dir.path(), "@").expect("run_describe failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("JJ:"));
        assert!(content.contains(">8"));
    }

    #[test]
    fn run_describe_roundtrips_description() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "my feature work").unwrap();
        let uri = run_describe(&jj, dir.path(), "@").expect("run_describe failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("my feature work"));
    }

    #[test]
    fn file_uri_uses_three_slashes_and_roundtrips() {
        // file:// URIs must have three slashes before the absolute path:
        // file:///abs/path on Unix, file:///C:/… on Windows. The naive
        // `format!("file://{}", path.display())` produced the right shape on
        // Unix by accident (path starts with "/") but emitted file://C:\… on
        // Windows, which VS Code couldn't open.
        let dir = tempdir().unwrap();
        let path = dir.path().join("foo.jujutsu");
        std::fs::write(&path, "hi").unwrap();
        let uri = file_uri(&path);
        assert!(
            uri.starts_with("file:///"),
            "expected three slashes; got: {uri}"
        );
        let parsed = path_from_uri(&uri).expect("uri should roundtrip");
        assert_eq!(parsed, path);
    }

    #[test]
    fn path_from_uri_rejects_non_file_scheme() {
        assert_eq!(path_from_uri("http://example.com/foo"), None);
        assert_eq!(path_from_uri("not a uri at all"), None);
        assert_eq!(path_from_uri(""), None);
    }

    #[test]
    fn run_refresh_with_garbage_uri_falls_back_to_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // A malformed URI shouldn't crash — earlier code panicked on Windows
        // because `Path::new("/C:/...")` after the naive strip wasn't a valid
        // path. With path_from_uri the unparseable case takes the status
        // fallback.
        let refreshed = run_refresh(&jj, dir.path(), "not a uri").unwrap();
        let content = std::fs::read_to_string(refreshed.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.contains("STATUS:"));
    }

    #[test]
    fn badjuju_dir_is_created() {
        let dir = tempdir().unwrap();
        let bd_dir = badjuju_dir(dir.path()).unwrap();
        assert!(bd_dir.exists());
        assert!(bd_dir.ends_with(".jj/badjuju"));
    }

    #[test]
    fn parse_describe_strips_jj_lines() {
        let content = "my feature\n\nJJ: ------------------------ >8 ------------------------\nJJ: Edit above\n";
        let result = parse_describe_content(content);
        assert_eq!(result, Some("my feature".to_string()));
    }

    #[test]
    fn parse_describe_returns_none_for_empty_content() {
        let content = "\n\nJJ: ------------------------ >8 ------------------------\n";
        assert_eq!(parse_describe_content(content), None);
    }

    #[test]
    fn parse_describe_returns_none_for_all_jj_lines() {
        let content = "JJ: some comment\nJJ: another\n";
        assert_eq!(parse_describe_content(content), None);
    }

    #[test]
    fn parse_log_revset_extracts_header() {
        let content = "REVSET: @ | @-\n\nsome log output";
        assert_eq!(parse_log_revset(content), Some("@ | @-".to_string()));
    }

    #[test]
    fn parse_log_revset_returns_none_for_missing_header() {
        let content = "no header here";
        assert_eq!(parse_log_revset(content), None);
    }

    #[test]
    fn parse_log_revset_returns_none_for_empty_revset() {
        let content = "REVSET: \n\nlog output";
        assert_eq!(parse_log_revset(content), None);
    }

    #[test]
    fn parse_log_revset_accepts_multiline_revset() {
        let content = "REVSET: @\n| @-\n| @--\n\nOUTPUT:\n\nlog output";
        assert_eq!(
            parse_log_revset(content),
            Some("@\n| @-\n| @--".to_string())
        );
    }

    #[test]
    fn parse_log_revset_skips_jj_comments_inside_block() {
        let content = "REVSET: @\nJJ: Mutable:  ancestors(reachable(@, mutable()), 2)\nJJ: Stack:    (immutable_heads()..@)::\n\nOUTPUT:\n\nlog output";
        assert_eq!(parse_log_revset(content), Some("@".to_string()));
    }

    #[test]
    fn parse_log_revset_skips_jj_comments_between_revset_lines() {
        let content =
            "REVSET: @\nJJ: a shortcut hint\n| @-\nJJ: another comment\n\nOUTPUT:\n\nlog output";
        assert_eq!(parse_log_revset(content), Some("@\n| @-".to_string()));
    }

    #[test]
    fn parse_log_revset_stops_at_blank_line() {
        let content = "REVSET: @\n| @-\n\n| @-- (should not be included)\nOUTPUT:\n\nlog output";
        assert_eq!(parse_log_revset(content), Some("@\n| @-".to_string()));
    }

    #[test]
    fn parse_log_revset_handles_only_continuation_lines() {
        // First REVSET: line is empty, but a continuation line provides the revset.
        let content = "REVSET:\n@ | @-\n\nOUTPUT:\n\nlog output";
        assert_eq!(parse_log_revset(content), Some("@ | @-".to_string()));
    }

    #[test]
    fn on_log_save_handles_multiline_revset() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let content = "REVSET: @\n| @-\n\nOUTPUT:\n\nstale";
        let uri = on_log_save(&jj, dir.path(), content).expect("on_log_save failed");
        let new_content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            new_content.starts_with("REVSET: @\n| @-\n"),
            "expected multi-line REVSET header to roundtrip, got:\n{new_content}"
        );
    }

    #[test]
    fn parse_describe_revision_extracts_header() {
        let content = "my new description\n\nJJ: separator\nJJ: revision: abc123\n";
        assert_eq!(parse_describe_revision(content), Some("abc123".to_string()));
    }

    #[test]
    fn parse_describe_revision_returns_none_for_missing_header() {
        let content = "no revision here\nJJ: only generic comment\n";
        assert_eq!(parse_describe_revision(content), None);
    }

    #[test]
    fn parse_describe_revision_returns_none_for_empty_value() {
        let content = "desc\nJJ: revision:   \n";
        assert_eq!(parse_describe_revision(content), None);
    }

    #[test]
    fn run_describe_targets_explicit_revision_and_embeds_header() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "parent description").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "child description").unwrap();
        let uri = run_describe(&jj, dir.path(), "@-").expect("run_describe failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(
            content.starts_with("parent description"),
            "expected parent desc in body; got:\n{content}"
        );
        assert!(
            content.contains("JJ: revision: @-"),
            "expected JJ: revision header; got:\n{content}"
        );
    }

    #[test]
    fn on_describe_save_routes_to_embedded_revision() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "parent v1").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "child v1").unwrap();
        // Save content with an explicit revision header pointing at @-.
        let content = "parent v2\n\nJJ: separator\nJJ: revision: @-\n";
        on_describe_save(&jj, dir.path(), content).expect("on_describe_save failed");
        let parent_desc = jj.describe_get("@-").unwrap();
        assert!(
            parent_desc.contains("parent v2"),
            "expected parent updated; got: {parent_desc}"
        );
        let at_desc = jj.describe_get("@").unwrap();
        assert!(
            at_desc.contains("child v1"),
            "expected @ untouched; got: {at_desc}"
        );
    }

    #[test]
    fn on_describe_save_applies_description() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let content = "new description\n\nJJ: separator\n";
        on_describe_save(&jj, dir.path(), content).expect("on_describe_save failed");
        let desc = jj.describe_get("@").unwrap();
        assert!(desc.contains("new description"));
    }

    #[test]
    fn on_describe_save_regenerates_log_when_present() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "before").unwrap();
        // Open a log window so log.jujutsu exists on disk with a known REVSET.
        run_log(&jj, dir.path(), "@").unwrap();
        let content = "after\n\nJJ: separator\nJJ: revision: @\n";
        on_describe_save(&jj, dir.path(), content).expect("on_describe_save failed");
        let log_path = dir.path().join(".jj/badjuju/log.jujutsu");
        let new_log = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            new_log.starts_with("REVSET: @"),
            "expected REVSET header preserved; got:\n{new_log}"
        );
        assert!(
            new_log.contains("after"),
            "expected refreshed log to show new description; got:\n{new_log}"
        );
        assert!(
            !new_log.contains("before"),
            "expected log no longer to show old description; got:\n{new_log}"
        );
    }

    #[test]
    fn on_describe_save_skips_log_regen_when_absent() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let content = "new desc\n\nJJ: separator\nJJ: revision: @\n";
        on_describe_save(&jj, dir.path(), content).expect("on_describe_save failed");
        let log_path = dir.path().join(".jj/badjuju/log.jujutsu");
        assert!(
            !log_path.exists(),
            "log.jujutsu should not be created when it didn't already exist"
        );
    }

    #[test]
    fn on_describe_save_skips_empty_content() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "original description").unwrap();
        let content = "\n\nJJ: separator\n";
        on_describe_save(&jj, dir.path(), content).expect("on_describe_save failed");
        let desc = jj.describe_get("@").unwrap();
        assert!(desc.contains("original description"));
    }

    #[test]
    fn run_diff_writes_file_with_revision_header() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "hello\n").unwrap();
        jj.describe_set("@", "add readme").unwrap();
        let uri = run_diff(&jj, dir.path(), "@").expect("run_diff failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(
            content.starts_with("REVISION: @\n"),
            "missing REVISION header:\n{content}"
        );
        assert!(
            content.contains("DIFF:"),
            "missing DIFF section:\n{content}"
        );
        assert!(
            content.contains("readme.txt"),
            "diff body should mention readme.txt:\n{content}"
        );
        assert!(
            content.contains("COMMAND REFERENCE:"),
            "missing command reference:\n{content}"
        );
    }

    #[test]
    fn run_diff_with_empty_revision_defaults_to_at() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_diff(&jj, dir.path(), "").expect("run_diff failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("REVISION: @\n"));
    }

    #[test]
    fn run_diff_writes_file_to_badjuju_dir() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_diff(&jj, dir.path(), "@").expect("run_diff failed");
        let path = uri.strip_prefix("file://").unwrap();
        assert!(
            path.ends_with(".jj/badjuju/diff.jujutsu"),
            "unexpected path: {path}"
        );
    }

    #[test]
    fn parse_diff_revision_extracts_header() {
        let content = "REVISION: abc123\n\nDIFF:\n...";
        assert_eq!(parse_diff_revision(content), Some("abc123".to_string()));
    }

    #[test]
    fn parse_diff_revision_returns_none_for_missing_header() {
        let content = "no header here\n";
        assert_eq!(parse_diff_revision(content), None);
    }

    #[test]
    fn parse_diff_revision_returns_none_for_empty_value() {
        let content = "REVISION:   \n";
        assert_eq!(parse_diff_revision(content), None);
    }

    #[test]
    fn run_refresh_with_diff_uri_regenerates_diff() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let diff_uri = run_diff(&jj, dir.path(), "@").unwrap();
        let refreshed = run_refresh(&jj, dir.path(), &diff_uri).expect("run_refresh failed");
        let content = std::fs::read_to_string(refreshed.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("REVISION: @"));
    }

    #[test]
    fn run_refresh_with_status_uri_regenerates_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let status_uri = run_status(&jj, dir.path()).unwrap();
        let refreshed = run_refresh(&jj, dir.path(), &status_uri).expect("run_refresh failed");
        assert!(refreshed.starts_with("file://"));
        let content = std::fs::read_to_string(refreshed.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.contains("STATUS:"));
    }

    #[test]
    fn run_refresh_with_log_uri_regenerates_log() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let log_uri = run_log(&jj, dir.path(), "@").unwrap();
        let refreshed = run_refresh(&jj, dir.path(), &log_uri).expect("run_refresh failed");
        assert!(refreshed.starts_with("file://"));
        let content = std::fs::read_to_string(refreshed.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.contains("REVSET:"));
    }

    #[test]
    fn run_refresh_with_empty_uri_falls_back_to_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_refresh(&jj, dir.path(), "").expect("run_refresh failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.contains("STATUS:"));
    }

    #[test]
    fn run_new_writes_status_and_returns_uri() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_new(&jj, dir.path(), "").expect("run_new failed");
        assert!(uri.starts_with("file://"));
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.contains("STATUS:"));
    }

    #[test]
    fn run_new_creates_new_change_in_log() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let log_before = jj.log("@").unwrap();
        run_new(&jj, dir.path(), "").expect("run_new failed");
        let log_after = jj.log("@").unwrap();
        assert_ne!(log_before, log_after);
    }

    /// When a parent revision is provided, the new change should be a child
    /// of that commit (and @ should move to the new change) rather than being
    /// a child of the previous @.
    #[test]
    fn run_new_with_explicit_parent_places_change_under_that_commit() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "child").unwrap();
        let parent_id = jj.change_ids("@-").unwrap().first().cloned().unwrap();
        let uri = run_new(&jj, dir.path(), &parent_id).expect("run_new failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("STATUS:"));
        let new_parent_ids = jj.change_ids("@-").unwrap();
        assert_eq!(
            new_parent_ids.first(),
            Some(&parent_id),
            "expected new @ to be a child of the explicit parent"
        );
    }

    #[test]
    fn run_squash_with_empty_file_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_squash(&jj, dir.path(), "", "").expect("run_squash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("MESSAGE: squash: no file selected"));
    }

    #[test]
    fn run_squash_moves_file_into_parent_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Parent commit has the file with one content.
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        // Working copy modifies the file.
        std::fs::write(dir.path().join("readme.txt"), "v2\n").unwrap();
        let uri = run_squash(&jj, dir.path(), "readme.txt", "").expect("run_squash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("STATUS:"));
        assert!(
            !content.contains("readme.txt"),
            "expected file squashed away"
        );
    }

    #[test]
    fn run_squash_reports_error_when_file_does_not_exist() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_squash(&jj, dir.path(), "does-not-exist.txt", "").expect("run_squash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: squash does-not-exist.txt from @ failed:"),
            "expected error message, got:\n{content}"
        );
    }

    #[test]
    fn run_unsquash_with_no_children_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        // @ has no children — unsquash should fail with descriptive message.
        let uri = run_unsquash(&jj, dir.path(), "readme.txt", "").expect("run_unsquash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: unsquash readme.txt from @: revision has 0 children"),
            "got:\n{content}"
        );
    }

    #[test]
    fn run_unsquash_with_empty_file_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_unsquash(&jj, dir.path(), "", "").expect("run_unsquash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("MESSAGE: unsquash: no file selected"));
    }

    #[test]
    fn run_unsquash_moves_file_to_child() {
        use std::process::Command;
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // @-: source commit with the file we want to "unsquash".
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("@", "source").unwrap();
        // Create a child commit so @ has exactly one child after we edit back.
        jj.new_change("").unwrap();
        jj.describe_set("@", "child").unwrap();
        // Move back to source so @ has the child we just created.
        Command::new("jj")
            .args(["edit", "@-"])
            .current_dir(dir.path())
            .output()
            .expect("jj edit failed");
        let uri = run_unsquash(&jj, dir.path(), "readme.txt", "").expect("run_unsquash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("STATUS:"));
    }

    #[test]
    fn run_unsquash_targets_explicit_revision() {
        // Set up parent (has the file) → @ (the working copy / child of parent).
        // Cursor in the stack section sits on parent's stat line. Unsquash with revision="@-"
        // should move the file from parent → @ (its only child).
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("@", "parent with the file").unwrap();
        jj.new_change("").unwrap();
        // @ is now a fresh child of parent. Parent has the only copy of readme.txt.
        let uri = run_unsquash(&jj, dir.path(), "readme.txt", "@-").expect("run_unsquash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("STATUS:"),
            "expected status (operation should have succeeded), got:\n{content}"
        );
        // After unsquash, @ now owns the file change rather than the parent.
        let status = jj.status().unwrap();
        assert!(
            status.contains("readme.txt"),
            "expected readme.txt in working copy after unsquash; status was:\n{status}"
        );
    }

    #[test]
    fn run_squash_targets_explicit_revision() {
        // parent (file) → middle (no diff) → @ (no diff).
        // Squash readme.txt from "middle" should move from middle → parent.
        // Since middle has no changes initially, set up so middle DOES have a change.
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Parent: empty.
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        // Middle (@): add readme.txt.
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("@", "middle").unwrap();
        jj.new_change("").unwrap();
        // @: now child of middle.
        let uri = run_squash(&jj, dir.path(), "readme.txt", "@-").expect("run_squash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("STATUS:"),
            "expected status, got:\n{content}"
        );
    }

    #[test]
    fn run_squash_with_explicit_revision_reports_parent_count() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // root() has 0 parents — squashing from root should report 0 parents.
        let uri = run_squash(&jj, dir.path(), "readme.txt", "root()").expect("run_squash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: squash readme.txt from root(): revision has 0 parents"),
            "got:\n{content}"
        );
    }

    #[test]
    fn run_push_with_no_remote_returns_status_uri() {
        // jj git push with no remote is a no-op (exits 0); run_push should return status URI.
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_push(&jj, dir.path(), false).expect("run_push failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("STATUS:"),
            "expected STATUS on successful no-op push, got:\n{content}"
        );
    }

    #[test]
    fn run_push_with_force_flag_also_returns_status_uri() {
        // force_with_lease=true still calls jj git push (jj has no --force-with-lease flag).
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_push(&jj, dir.path(), true).expect("run_push with force_flag failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("STATUS:"),
            "expected STATUS on push with force flag, got:\n{content}"
        );
    }

    #[test]
    fn run_fetch_with_no_remote_reports_error_in_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_fetch(&jj, dir.path()).expect("run_fetch should produce a URI even on error");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: fetch failed:"),
            "expected error MESSAGE prelude, got:\n{content}"
        );
    }

    #[test]
    fn run_edit_moves_at_to_revision_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "child").unwrap();
        let parent_id = jj.change_ids("@-").unwrap().first().cloned().unwrap();
        let uri = run_edit(&jj, dir.path(), &parent_id).expect("run_edit failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("STATUS:"));
        let desc = jj.describe_get("@").unwrap();
        assert!(
            desc.contains("parent"),
            "expected @ to be on parent after edit; got: {desc}"
        );
    }

    #[test]
    fn run_edit_with_invalid_revision_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_edit(&jj, dir.path(), "not-a-real-change")
            .expect("run_edit should still produce a status URI on error");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: edit not-a-real-change failed:"),
            "expected error MESSAGE prelude, got:\n{content}"
        );
    }

    #[test]
    fn run_undo_reverts_last_operation_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "first").unwrap();
        jj.describe_set("@", "second").unwrap();
        let uri = run_undo(&jj, dir.path()).expect("run_undo failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("STATUS:"));
        let desc = jj.describe_get("@").unwrap();
        assert!(
            desc.contains("first"),
            "expected undo to roll back to first; got: {desc}"
        );
    }

    #[test]
    fn run_abandon_abandons_explicit_revision_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Create stack: parent → middle → @. Abandon middle.
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "middle to abandon").unwrap();
        jj.new_change("").unwrap();
        let middle_id = jj.change_ids("@-").unwrap().first().cloned().unwrap();
        let uri = run_abandon(&jj, dir.path(), &middle_id).expect("run_abandon failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("STATUS:"));
        let log = jj.log("::@").unwrap();
        assert!(
            !log.contains("middle to abandon"),
            "expected middle change abandoned; log still shows it:\n{log}"
        );
    }

    #[test]
    fn run_abandon_with_invalid_revision_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_abandon(&jj, dir.path(), "not-a-real-change")
            .expect("run_abandon should still produce a status URI on error");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: abandon not-a-real-change failed:"),
            "expected error MESSAGE prelude, got:\n{content}"
        );
    }

    #[test]
    fn run_abandon_with_empty_revision_defaults_to_at() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "a description").unwrap();
        let uri = run_abandon(&jj, dir.path(), "").expect("run_abandon failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("STATUS:"));
        // After abandoning @, the new working copy should be empty (no description carried over).
        let desc = jj.describe_get("@").unwrap();
        assert!(
            !desc.contains("a description"),
            "expected @ abandoned, but description survived: {desc}"
        );
    }

    #[test]
    fn run_abandon_preserves_stat_state() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        run_toggle_stat(&jj, dir.path()).unwrap();
        jj.describe_set("@", "to abandon").unwrap();
        let uri = run_abandon(&jj, dir.path(), "@").expect("run_abandon failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            read_current_stat(dir.path()),
            "abandon should preserve stat=on"
        );
        assert!(
            !content.contains("STATS:"),
            "status buffer must not leak STATS marker:\n{content}"
        );
    }

    #[test]
    fn run_undo_preserves_stat_state() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        run_toggle_stat(&jj, dir.path()).unwrap();
        jj.describe_set("@", "a description").unwrap();
        run_undo(&jj, dir.path()).expect("run_undo failed");
        assert!(
            read_current_stat(dir.path()),
            "undo should preserve stat=on"
        );
    }

    #[test]
    fn run_status_defaults_to_stat_off() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_status(&jj, dir.path()).unwrap();
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            !read_current_stat(dir.path()),
            "expected stat off by default"
        );
        assert!(
            !content.contains("STATS:"),
            "status buffer must not contain STATS marker:\n{content}"
        );
    }

    #[test]
    fn toggle_stat_flips_state() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        run_status(&jj, dir.path()).unwrap();
        run_toggle_stat(&jj, dir.path()).unwrap();
        assert!(
            read_current_stat(dir.path()),
            "expected stat=on after toggle"
        );
        run_toggle_stat(&jj, dir.path()).unwrap();
        assert!(
            !read_current_stat(dir.path()),
            "expected stat=off after second toggle"
        );
    }

    #[test]
    fn run_status_preserves_stat_across_calls() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        run_toggle_stat(&jj, dir.path()).unwrap(); // stat on
        run_status(&jj, dir.path()).unwrap();
        assert!(
            read_current_stat(dir.path()),
            "stat should be preserved across status calls"
        );
    }

    #[test]
    fn squash_preserves_stat_state() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        std::fs::write(dir.path().join("readme.txt"), "v2\n").unwrap();
        run_toggle_stat(&jj, dir.path()).unwrap(); // stat on
        run_squash(&jj, dir.path(), "readme.txt", "").unwrap();
        assert!(
            read_current_stat(dir.path()),
            "squash should preserve stat=on"
        );
    }

    #[test]
    fn run_next_with_no_descendants_reports_error_in_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Fresh repo: @ has no descendants. `jj next` should fail and surface
        // the error as a MESSAGE prelude rather than propagating it.
        let uri = run_next(&jj, dir.path(), false).expect("run_next failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: next failed:"),
            "expected MESSAGE prelude on next failure, got:\n{content}"
        );
    }

    #[test]
    fn run_prev_moves_working_copy_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "child").unwrap();
        let uri = run_prev(&jj, dir.path(), false).expect("run_prev failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("STATUS:"),
            "expected status, got:\n{content}"
        );
    }

    #[test]
    fn run_prev_with_edit_moves_at_to_parent() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "child").unwrap();
        run_prev(&jj, dir.path(), true).expect("run_prev edit failed");
        let desc = jj.describe_get("@").unwrap();
        assert!(
            desc.contains("parent"),
            "expected @ on parent after prev --edit; got: {desc}"
        );
    }

    #[test]
    fn run_next_preserves_stat_state_on_failure() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        run_toggle_stat(&jj, dir.path()).unwrap(); // stat on
        // Fresh repo's @ has no descendants → next fails → status is rendered
        // with stat preserved.
        run_next(&jj, dir.path(), false).expect("run_next failed");
        assert!(
            read_current_stat(dir.path()),
            "next failure should preserve stat=on"
        );
    }

    #[test]
    fn on_log_save_regenerates_with_new_revset() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let content = "REVSET: @\n\nold log output";
        let uri = on_log_save(&jj, dir.path(), content).expect("on_log_save failed");
        let path = uri.strip_prefix("file://").unwrap();
        let new_content = std::fs::read_to_string(path).unwrap();
        assert!(new_content.starts_with("REVSET: @"));
    }

    #[test]
    fn run_rebase_moves_commit_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // root → A (@) → B. We rebase B onto root.
        jj.describe_set("@", "commit A").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "commit B").unwrap();
        let b_id = jj.change_ids("@").unwrap().first().cloned().unwrap();
        let root_id = jj.change_ids("root()").unwrap().first().cloned().unwrap();
        let uri = run_rebase(&jj, dir.path(), &b_id, &root_id).expect("run_rebase failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("STATUS:"),
            "expected STATUS: header, got:\n{content}"
        );
    }

    #[test]
    fn run_rebase_with_empty_dest_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_rebase(&jj, dir.path(), "@", "")
            .expect("run_rebase with empty dest should return a URI");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: rebase: destination revision required"),
            "expected error MESSAGE, got:\n{content}"
        );
    }

    #[test]
    fn run_rebase_with_invalid_dest_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_rebase(&jj, dir.path(), "@", "not-a-real-rev")
            .expect("run_rebase should still produce a URI on error");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: rebase @ to not-a-real-rev failed:"),
            "expected error MESSAGE prelude, got:\n{content}"
        );
    }

    #[test]
    fn run_bookmark_create_returns_status_uri() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_bookmark(&jj, dir.path(), "create", "mymark", "@")
            .expect("run_bookmark create failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("STATUS:"),
            "expected STATUS: header, got:\n{content}"
        );
    }

    #[test]
    fn run_bookmark_with_empty_name_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_bookmark(&jj, dir.path(), "create", "", "@")
            .expect("run_bookmark with empty name should return a URI");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: bookmark: name is required"),
            "expected error MESSAGE, got:\n{content}"
        );
    }

    #[test]
    fn run_bookmark_unknown_sub_action_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_bookmark(&jj, dir.path(), "nope", "mymark", "@")
            .expect("run_bookmark with bad sub-action should return a URI");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: bookmark: unknown sub-action 'nope'"),
            "expected error MESSAGE, got:\n{content}"
        );
    }

    #[test]
    fn run_bookmark_delete_removes_bookmark_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.bookmark_create("toremove", "@").unwrap();
        let uri = run_bookmark(&jj, dir.path(), "delete", "toremove", "")
            .expect("run_bookmark delete failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("STATUS:"),
            "expected STATUS: header, got:\n{content}"
        );
    }

    #[test]
    fn run_bookmark_move_updates_target_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.bookmark_create("moving", "@").unwrap();
        jj.new_change("").unwrap();
        let uri =
            run_bookmark(&jj, dir.path(), "move", "moving", "@").expect("run_bookmark move failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("STATUS:"),
            "expected STATUS: header, got:\n{content}"
        );
    }

    // --- Cursor-arg resolver tests ----------------------------------------

    fn no_docs(_uri: &str) -> Option<String> {
        None
    }

    #[test]
    fn resolve_revision_arg_no_arg_returns_empty() {
        let r = resolve_revision_arg(None, no_docs).unwrap();
        assert_eq!(r, "");
    }

    #[test]
    fn resolve_revision_arg_string_arg_returns_string() {
        let v = serde_json::json!("abc123");
        let r = resolve_revision_arg(Some(&v), no_docs).unwrap();
        assert_eq!(r, "abc123");
    }

    #[test]
    fn resolve_revision_arg_null_arg_returns_empty() {
        let v = serde_json::Value::Null;
        let r = resolve_revision_arg(Some(&v), no_docs).unwrap();
        assert_eq!(r, "");
    }

    #[test]
    fn resolve_revision_arg_cursor_form_resolves_log_commit() {
        let log = ["REVSET: @", "", "OUTPUT:", "", "@  qpvuntsm 1234abcd"].join("\n");
        let arg = serde_json::json!({
            "cursor": { "uri": "file:///x/log.jujutsu", "line": 4 }
        });
        let r = resolve_revision_arg(Some(&arg), |uri| {
            (uri == "file:///x/log.jujutsu").then(|| log.clone())
        })
        .unwrap();
        assert_eq!(r, "qpvuntsm");
    }

    #[test]
    fn resolve_revision_arg_cursor_form_invalid_shape_errors() {
        let arg = serde_json::json!({ "cursor": { "uri": "file:///x/log.jujutsu" } });
        let err = resolve_revision_arg(Some(&arg), no_docs).unwrap_err();
        assert!(matches!(err, CursorResolveError::InvalidArg));
    }

    #[test]
    fn resolve_revision_arg_cursor_form_unsupported_buffer_errors() {
        let arg = serde_json::json!({
            "cursor": { "uri": "file:///x/other.txt", "line": 0 }
        });
        let err = resolve_revision_arg(Some(&arg), |_| Some(String::new())).unwrap_err();
        assert!(matches!(err, CursorResolveError::UnsupportedBuffer(_)));
    }

    #[test]
    fn resolve_revision_arg_cursor_form_doc_missing_errors() {
        let arg = serde_json::json!({
            "cursor": { "uri": "file:///x/log.jujutsu", "line": 0 }
        });
        let err = resolve_revision_arg(Some(&arg), no_docs).unwrap_err();
        assert!(matches!(err, CursorResolveError::DocNotFound(_)));
    }

    #[test]
    fn resolve_revision_arg_cursor_form_no_commit_errors() {
        let log = "REVSET: @\n".to_string();
        let arg = serde_json::json!({
            "cursor": { "uri": "file:///x/log.jujutsu", "line": 0 }
        });
        let err = resolve_revision_arg(Some(&arg), |_| Some(log.clone())).unwrap_err();
        assert!(matches!(err, CursorResolveError::NoRevisionAtCursor));
    }

    #[test]
    fn resolve_file_and_revision_arg_string_args_return_pair() {
        let file = serde_json::json!("readme.txt");
        let rev = serde_json::json!("@-");
        let (f, r) = resolve_file_and_revision_arg(Some(&file), Some(&rev), no_docs).unwrap();
        assert_eq!(f, "readme.txt");
        assert_eq!(r, "@-");
    }

    #[test]
    fn resolve_file_and_revision_arg_no_args_errors() {
        let err = resolve_file_and_revision_arg(None, None, no_docs).unwrap_err();
        assert!(matches!(err, CursorResolveError::InvalidArg));
    }

    #[test]
    fn resolve_file_and_revision_arg_lone_file_string_errors() {
        // Single string arg (no second arg) isn't the legacy 2-string form
        // and isn't cursor form, so it should error rather than guess.
        let v = serde_json::json!("foo.rs");
        let err = resolve_file_and_revision_arg(Some(&v), None, no_docs).unwrap_err();
        assert!(matches!(err, CursorResolveError::InvalidArg));
    }

    #[test]
    fn resolve_file_and_revision_arg_cursor_form_resolves_both() {
        let status = [
            "STATUS:",
            "",
            "M src/main.rs",
            "",
            "STACK: @",
            "",
            "@  qpvuntsm 1234abcd",
        ]
        .join("\n");
        let arg = serde_json::json!({
            "cursor": { "uri": "file:///x/status.jujutsu", "line": 2 }
        });
        let (file, rev) =
            resolve_file_and_revision_arg(Some(&arg), None, |_| Some(status.clone())).unwrap();
        assert_eq!(file, "src/main.rs");
        // File lines belong to the working copy → "@"
        assert_eq!(rev, "@");
    }

    #[test]
    fn resolve_log_shortcut_arg_string_returns_none() {
        let v = serde_json::json!("foo");
        let r = resolve_log_shortcut_arg(Some(&v), no_docs).unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn resolve_log_shortcut_arg_cursor_form_resolves_revset() {
        let log = ["REVSET: @", "JJ: Mutable: ancestors(@)", ""].join("\n");
        let arg = serde_json::json!({
            "cursor": { "uri": "file:///x/log.jujutsu", "line": 1 }
        });
        let r = resolve_log_shortcut_arg(Some(&arg), |_| Some(log.clone()))
            .unwrap()
            .unwrap();
        assert_eq!(r, "ancestors(@)");
    }

    #[test]
    fn resolve_log_shortcut_arg_non_shortcut_line_errors() {
        let log = ["REVSET: @", "OUTPUT:", "@  qpvuntsm 1234"].join("\n");
        let arg = serde_json::json!({
            "cursor": { "uri": "file:///x/log.jujutsu", "line": 2 }
        });
        let err = resolve_log_shortcut_arg(Some(&arg), |_| Some(log.clone())).unwrap_err();
        assert!(matches!(err, CursorResolveError::NoShortcutAtCursor));
    }

    #[test]
    fn resolve_log_shortcut_arg_non_log_buffer_errors() {
        let arg = serde_json::json!({
            "cursor": { "uri": "file:///x/status.jujutsu", "line": 0 }
        });
        let err = resolve_log_shortcut_arg(Some(&arg), |_| Some(String::new())).unwrap_err();
        assert!(matches!(err, CursorResolveError::UnsupportedBuffer(_)));
    }
}
