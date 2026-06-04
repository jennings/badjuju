//! Line-positional parsing helpers for the generated `*.jujutsu` buffers.
//!
//! Mirrors the client-side parsers in `clients/vscode/src/extension.ts` and
//! `clients/neovim/lua/badjuju/{parse,log_shortcut}.lua`. The server already
//! has full document text in `State.documents`, so resolving "what is at line
//! N" can move here. Clients then ship a cursor position to the server
//! instead of pre-resolving — the design described in the plan.
//!
//! This module is intentionally pure: no I/O, no `jj` subprocess. All
//! functions accept the buffer text and return `Option`s so callers can
//! decide how to surface "nothing at the cursor" (e.g., as a clear LSP error).

/// Which kind of generated buffer the cursor sits in. Detected from the URI
/// filename.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferKind {
    Status,
    Log,
    Diff,
    Squash,
    /// Reusable single-instance `hunk-edit.jujutsu` buffer used by the squash
    /// `edit hunk` flow (and, in the future, `jj diffedit`-style flows).
    HunkEdit,
}

impl BufferKind {
    /// Detect kind from a URI string by trailing filename. Returns `None` for
    /// URIs that don't end in one of the known buffer names.
    pub fn from_uri(uri: &str) -> Option<Self> {
        // Check the hunk-edit URI first so the generic "ends with .jujutsu"
        // filename match doesn't claim it.
        if is_hunk_edit_uri(uri) {
            Some(Self::HunkEdit)
        } else if uri.ends_with("status.jujutsu") {
            Some(Self::Status)
        } else if uri.ends_with("log.jujutsu") {
            Some(Self::Log)
        } else if is_diff_uri(uri) {
            Some(Self::Diff)
        } else if is_squash_uri(uri) {
            Some(Self::Squash)
        } else {
            None
        }
    }
}

/// True for the single `.jj/badjuju/hunk-edit.jujutsu` URI. Lives directly
/// under `badjuju/` (not under `squash/`), so other-kind detection won't claim
/// it.
pub fn is_hunk_edit_uri(uri: &str) -> bool {
    uri.ends_with("/.jj/badjuju/hunk-edit.jujutsu")
}

/// Whether the URI resolves to a squash window file: `.jj/badjuju/squash/<id>-<id>.jujutsu`.
fn is_squash_uri(uri: &str) -> bool {
    let name = uri.rsplit('/').next().unwrap_or(uri);
    // Must look like "<12chars>-<12chars>.jujutsu" and the containing path
    // must include a "/squash/" segment.
    if !uri.contains("/squash/") {
        return false;
    }
    // Filename: two 12-char lowercase alphanumeric sequences joined by '-', ending in .jujutsu.
    let Some(stem) = name.strip_suffix(".jujutsu") else {
        return false;
    };
    let Some(mid) = stem.find('-') else {
        return false;
    };
    let (left, right) = (&stem[..mid], &stem[mid + 1..]);
    !left.is_empty()
        && !right.is_empty()
        && left
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && right
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

fn is_diff_uri(uri: &str) -> bool {
    let name = uri.rsplit('/').next().unwrap_or(uri);
    if name == "diff.jujutsu" {
        return true;
    }
    for prefix in ["diff-change-", "diff-commit-"] {
        if let Some(rest) = name.strip_prefix(prefix)
            && let Some(id) = rest.strip_suffix(".jujutsu")
            && !id.is_empty()
        {
            return true;
        }
    }
    false
}

/// A `JJ: <Label>: <revset>` shortcut line in `log.jujutsu`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogShortcut {
    pub label: String,
    pub revset: String,
}

/// Resolve the revision the cursor refers to, given the buffer's full text,
/// a 0-indexed line, and which kind of buffer it is.
///
/// - `Status`: stat-format file lines (`M src/main.rs` etc.) belong to the
///   working copy. Otherwise walk upward looking for a commit header; if no
///   commit header is found before the top of the buffer, fall back to `@`.
/// - `Log`: walk upward looking for a commit header; return `None` if the
///   cursor is above every commit line (e.g., in the `REVSET:` header).
/// - `Diff`: a diff buffer always represents a single revision named in its
///   `REVISION:` header; the cursor position doesn't matter.
pub fn revision_at_line(content: &str, line: usize, kind: BufferKind) -> Option<String> {
    match kind {
        BufferKind::Status => Some(revision_at_line_status(content, line)),
        BufferKind::Log => revision_at_line_log(content, line),
        BufferKind::Diff => revision_from_diff_header(content),
        BufferKind::Squash => revision_from_squash_header(content),
        // The hunk-edit buffer is action-oriented; no enclosing revision.
        BufferKind::HunkEdit => None,
    }
}

/// Which CHANGES section a squash-buffer cursor line falls in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SquashSection {
    Selected,
    Remaining,
}

/// Resolve which section (SELECTED or REMAINING CHANGES) the cursor is in for
/// a squash window buffer. Returns `None` if the line is above both sections.
pub fn squash_section_at_line(content: &str, line: usize) -> Option<SquashSection> {
    let lines: Vec<&str> = content.lines().collect();
    let start = line.min(lines.len().saturating_sub(1));
    for i in (0..=start).rev() {
        if lines[i] == "REMAINING CHANGES:" {
            return Some(SquashSection::Remaining);
        }
        if lines[i] == "SELECTED CHANGES:" {
            return Some(SquashSection::Selected);
        }
    }
    None
}

fn revision_at_line_status(content: &str, line: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let current = lines.get(line).copied().unwrap_or("");
    if parse_status_header_line(current).is_some() {
        return "@".to_string();
    }
    if let Some(rev) = status_summary_header_revset(current) {
        return rev;
    }
    let start = if line >= lines.len() {
        lines.len().saturating_sub(1)
    } else {
        line
    };
    if lines.is_empty() {
        return "@".to_string();
    }
    for i in (0..=start).rev() {
        let text = lines[i];
        if let Some(change_id) = match_commit_header(text) {
            return change_id.to_string();
        }
        if text.starts_with("WORKING COPY CHANGES (") {
            return "@".to_string();
        }
        if let Some(parent_id) = parse_parent_changes_header(text) {
            return parent_id.to_string();
        }
    }
    "@".to_string()
}

fn revision_at_line_log(content: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }
    let start = if line >= lines.len() {
        lines.len() - 1
    } else {
        line
    };
    for i in (0..=start).rev() {
        if let Some(change_id) = match_commit_header(lines[i]) {
            return Some(change_id.to_string());
        }
    }
    None
}

fn revision_from_squash_header(content: &str) -> Option<String> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("From:") {
            let rest = rest.trim();
            // "From: <change_id_short> <commit_id_short> <desc>" — first word is the change-id
            let change_id = rest.split_whitespace().next()?;
            if !change_id.is_empty() {
                return Some(change_id.to_string());
            }
        }
    }
    None
}

fn revision_from_diff_header(content: &str) -> Option<String> {
    let first = content.lines().next()?;
    // Handle new-style CHANGE_ID: and COMMIT_ID: headers as well as the legacy
    // REVISION: form. All three contain a jj-usable revision expression.
    for prefix in ["CHANGE_ID:", "COMMIT_ID:", "REVISION:"] {
        if let Some(rest) = first.strip_prefix(prefix) {
            let id = rest.trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

/// Which section of a `status.jujutsu` buffer a cursor position resolves to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorTarget {
    /// File in the WORKING COPY CHANGES block — belongs to `@`.
    WorkingCopyFile { path: String },
    /// File in a PARENT CHANGES block — belongs to the named parent change-id.
    ParentFile { parent_id: String, path: String },
    /// File in the STACK section — belongs to the named change-id from `jj log --stat`.
    StackCommitFile { change_id: String, path: String },
}

/// Resolve the full cursor target (section + file) for a 0-indexed line in a
/// `status.jujutsu` buffer. Returns `None` when the line does not resolve to a
/// file (blank line, section header, description, etc.).
pub fn cursor_target_at_line(content: &str, line: usize) -> Option<CursorTarget> {
    let file = file_at_line(content, line)?;
    let lines: Vec<&str> = content.lines().collect();
    let start = line.min(lines.len().saturating_sub(1));
    for i in (0..=start).rev() {
        let text = lines[i];
        if text.starts_with("WORKING COPY CHANGES (") {
            return Some(CursorTarget::WorkingCopyFile { path: file });
        }
        if let Some(parent_id) = parse_parent_changes_header(text) {
            return Some(CursorTarget::ParentFile {
                parent_id: parent_id.to_string(),
                path: file,
            });
        }
        if let Some(change_id) = match_commit_header(text) {
            return Some(CursorTarget::StackCommitFile {
                change_id: change_id.to_string(),
                path: file,
            });
        }
        if text.starts_with("STACK:") || text.starts_with("COMMAND REFERENCE:") {
            return None;
        }
    }
    None
}

/// A file+hunk identified at a cursor position in a squash window buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SquashHunk {
    pub file: String,
    pub header: String,
    pub content: String,
}

/// True for lines that are plain file-path lines in a squash window section
/// (not blank, not hunk markers, not section/header lines).
fn is_squash_file_line(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    if line.starts_with("@@")
        || line.starts_with('+')
        || line.starts_with('-')
        || line.starts_with(' ')
    {
        return false;
    }
    // Section headers and SQUASHING header lines
    for prefix in [
        "SQUASHING:",
        "From:",
        "To:  ",
        "SELECTED CHANGES:",
        "REMAINING CHANGES:",
        "COMMAND REFERENCE:",
        "SQUASH TARGET",
    ] {
        if line.starts_with(prefix) {
            return false;
        }
    }
    true
}

/// Resolve the file path at a cursor position inside a squash window buffer.
/// Returns `Some(file_path)` when the line is a plain file-path line, or when
/// it is a hunk line (walks up to find the enclosing file). Returns `None` for
/// section headers, blank lines, and lines outside both squash sections.
pub fn squash_file_at_line(content: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let target = *lines.get(line)?;

    if is_squash_file_line(target) {
        return Some(target.trim_end().to_string());
    }

    // Hunk header or content: walk upward to find the enclosing file line.
    let is_hunk_line = target.starts_with("@@")
        || target.starts_with('+')
        || target.starts_with('-')
        || target.starts_with(' ');
    if is_hunk_line {
        for i in (0..line).rev() {
            let l = lines[i];
            if is_squash_file_line(l) {
                return Some(l.trim_end().to_string());
            }
            if l == "SELECTED CHANGES:" || l == "REMAINING CHANGES:" || l.is_empty() {
                break;
            }
        }
    }

    None
}

/// Resolve the hunk (file + `@@` header + content) at a cursor position
/// inside a squash window buffer. Returns `Some` when the cursor is on a hunk
/// header (`@@`) or hunk content line. Returns `None` for file-path lines,
/// section headers, and blank lines.
pub fn squash_hunk_at_line(content: &str, line: usize) -> Option<SquashHunk> {
    let lines: Vec<&str> = content.lines().collect();
    let target = *lines.get(line)?;

    // Determine which line is the @@ header for this cursor position.
    let hunk_header_line = if target.starts_with("@@") {
        line
    } else if target.starts_with('+') || target.starts_with('-') || target.starts_with(' ') {
        // Walk backward to the @@ header of this hunk.
        (0..line).rev().find(|&i| lines[i].starts_with("@@"))?
    } else {
        return None;
    };

    let header = lines[hunk_header_line].to_string();

    // Walk backward from the @@ line to find the enclosing file-path line.
    let file = (0..hunk_header_line).rev().find_map(|i| {
        let l = lines[i];
        if is_squash_file_line(l) {
            Some(l.trim_end().to_string())
        } else {
            None
        }
    })?;

    // Collect content lines (everything after @@ until blank or non-content line).
    let content_start = hunk_header_line + 1;
    let content_end = (content_start..lines.len())
        .find(|&i| {
            let l = lines[i];
            l.is_empty()
                || l.starts_with("@@")
                || (!l.starts_with('+') && !l.starts_with('-') && !l.starts_with(' '))
        })
        .unwrap_or(lines.len());

    let hunk_content = lines[content_start..content_end].join("\n");

    Some(SquashHunk {
        file,
        header,
        content: hunk_content,
    })
}

/// Resolve the file path at a given 0-indexed line of a `status.jujutsu`
/// buffer. Handles:
/// - `M src/main.rs` style (status flag lines)
/// - `│  src/main.rs | 3 +++` style (jj log --stat lines)
/// - Flush-left plain paths inside WORKING COPY CHANGES / PARENT CHANGES sections
/// - `@@` hunk headers and diff content lines: walks up to the enclosing plain
///   path line within the same CHANGES section
///
/// Renames rendered as `old => new` return only the destination path.
///
/// Returns `None` for blank lines, section header lines, the stat summary
/// line, and lines outside all recognisable file contexts.
pub fn file_at_line(content: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let line_text = lines.get(line).copied()?;

    // Existing formats: M/A/D prefix and stat lines.
    if let Some(p) = parse_file_line(line_text) {
        return Some(p);
    }

    // Diff hunk lines (@@) or add/remove/context lines inside a CHANGES block:
    // walk upward to the nearest plain path line in the same section, skipping
    // hunk markers and diff context lines (space-prefixed unchanged lines).
    if is_diff_hunk_line(line_text) || line_text.starts_with(' ') {
        for i in (0..line).rev() {
            let text = lines[i];
            if is_diff_hunk_line(text) || text.starts_with(' ') {
                continue;
            }
            // Hit a plain path line?
            if let Some(p) = parse_changes_file_line(text) {
                return Some(p);
            }
            // Hit a section header or section boundary — stop.
            break;
        }
        return None;
    }

    // Plain flush-left paths inside CHANGES sections.
    if is_in_changes_section(&lines, line) {
        return parse_changes_file_line(line_text);
    }

    None
}

/// Resolve a `JJ: <Label>: <revset>` shortcut at the given 0-indexed line of
/// a `log.jujutsu` buffer. Returns `None` if the line isn't a shortcut line.
pub fn log_shortcut_at_line(content: &str, line: usize) -> Option<LogShortcut> {
    let line_text = content.lines().nth(line)?;
    parse_log_shortcut(line_text)
}

/// Resolve the change_id when the cursor line IS itself a commit-header line
/// (graph char + change_id). Unlike [`revision_at_line`], this does *not*
/// walk upward — it returns `None` when the cursor sits on a description /
/// file / header line. Useful when a buffer mixes commit-headers with other
/// actionable line kinds (e.g., file lines in `status.jujutsu`).
pub fn commit_id_at_line(content: &str, line: usize) -> Option<String> {
    let line_text = content.lines().nth(line)?;
    match_commit_header(line_text).map(|s| s.to_string())
}

/// Uniform cursor → file target resolver for `Status`, `Log`, `Diff`, and
/// `Squash` buffers. Returns the repo-relative path, the enclosing revision,
/// and (when the cursor sits inside a unified-diff hunk) the corresponding
/// 1-indexed line in the new-file side of that hunk.
///
/// `on_minus_line` is `true` only when the cursor row is a `-` deletion row;
/// `line_in_file` is then the next slot in the new file where the deletion
/// would land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileCursorTarget {
    /// Repo-relative path to the file.
    pub path: String,
    /// Revision expression the cursor is "inside" (`@`, `@-`, change-id, etc.).
    pub revision: String,
    /// 1-indexed line in the file on the *new* side of the hunk. `None` when
    /// the cursor is on a bare filename row with no enclosing hunk header.
    pub line_in_file: Option<u32>,
    /// `true` iff the cursor row is a `-` deletion line.
    pub on_minus_line: bool,
}

/// Resolve the file target at a 0-indexed cursor line.
///
/// Returns `None` when the cursor doesn't sit on a file row, a hunk header,
/// or a hunk content row — e.g., blank lines, section headers, commit-info
/// lines, the `COMMAND REFERENCE` block, or hunk-edit buffers.
pub fn file_target_at_line(
    content: &str,
    line: usize,
    kind: BufferKind,
) -> Option<FileCursorTarget> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return None;
    }
    let target_line = line.min(lines.len() - 1);

    let path = file_path_for_kind(content, &lines, target_line, kind)?;
    let revision = revision_at_line(content, target_line, kind)?;
    let (line_in_file, on_minus_line) = hunk_line_in_file(&lines, target_line);

    Some(FileCursorTarget {
        path,
        revision,
        line_in_file,
        on_minus_line,
    })
}

fn file_path_for_kind(
    content: &str,
    lines: &[&str],
    target_line: usize,
    kind: BufferKind,
) -> Option<String> {
    match kind {
        BufferKind::Status | BufferKind::Log => file_at_line(content, target_line),
        BufferKind::Diff => diff_file_at_line(lines, target_line),
        BufferKind::Squash => squash_file_at_line(content, target_line),
        BufferKind::HunkEdit => None,
    }
}

/// Walk up to the nearest `+++ b/<path>` line. `/dev/null` (deleted file)
/// returns `None` — the buffer's revision doesn't contain a meaningful
/// source for that path.
fn diff_file_at_line(lines: &[&str], target_line: usize) -> Option<String> {
    for i in (0..=target_line).rev() {
        let l = lines[i];
        if let Some(rest) = l.strip_prefix("+++ ") {
            let path = rest.trim().strip_prefix("b/")?;
            if path == "/dev/null" {
                return None;
            }
            return Some(path.to_string());
        }
    }
    None
}

/// Resolve the (line_in_file, on_minus_line) pair for a cursor inside a
/// unified-diff hunk. Walks up from `target_line` looking for the enclosing
/// `@@ ... +<new_start>[,<n>] @@` header; returns `(None, false)` when the
/// cursor isn't sitting in a hunk (e.g., a bare filename row, blank line,
/// section header).
fn hunk_line_in_file(lines: &[&str], target_line: usize) -> (Option<u32>, bool) {
    let mut header_line: Option<usize> = None;
    let mut header_new_start: u32 = 0;
    for i in (0..=target_line).rev() {
        let l = lines[i];
        if let Some(new_start) = parse_hunk_header_new_start(l) {
            header_line = Some(i);
            header_new_start = new_start;
            break;
        }
        // Hit a non-hunk-body line before finding @@ — cursor isn't in a hunk.
        if is_hunk_boundary_line(l) {
            return (None, false);
        }
    }
    let header_line = match header_line {
        Some(h) => h,
        None => return (None, false),
    };

    let target = lines[target_line];
    if target_line == header_line {
        return (Some(header_new_start), false);
    }
    match target.chars().next() {
        Some('+') | Some(' ') => {
            let count = lines[header_line + 1..=target_line]
                .iter()
                .filter(|l| matches!(l.chars().next(), Some('+') | Some(' ')))
                .count() as u32;
            let line_in_file = header_new_start.saturating_add(count).saturating_sub(1);
            (Some(line_in_file), false)
        }
        Some('-') => {
            let count = lines[header_line + 1..target_line]
                .iter()
                .filter(|l| matches!(l.chars().next(), Some('+') | Some(' ')))
                .count() as u32;
            (Some(header_new_start.saturating_add(count)), true)
        }
        _ => (None, false),
    }
}

/// Returns `true` when this line terminates the walk-up for an enclosing
/// hunk header — i.e., a non-hunk-body line. Hunk body lines start with
/// `@@`, ` `, `+`, or `-`, except for `+++ ` / `--- ` file headers which
/// *are* boundaries.
fn is_hunk_boundary_line(line: &str) -> bool {
    if line.is_empty() {
        return true;
    }
    if line.starts_with("@@") {
        return false;
    }
    if line.starts_with("+++") || line.starts_with("---") {
        return true;
    }
    if line.starts_with('+') || line.starts_with('-') || line.starts_with(' ') {
        return false;
    }
    true
}

/// Parse the `+<new_start>` portion of a `@@ -OLD +NEW @@` hunk header.
/// Returns `None` for lines that don't begin with `@@` or whose new-side
/// numeric portion can't be parsed.
fn parse_hunk_header_new_start(line: &str) -> Option<u32> {
    if !line.starts_with("@@") {
        return None;
    }
    let rest = &line[2..];
    let plus_pos = rest.find(" +")?;
    let after = &rest[plus_pos + 2..];
    let num_end = after
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after.len());
    if num_end == 0 {
        return None;
    }
    after[..num_end].parse().ok()
}

/// Recognise a status-buffer top-section "summary header" line of the form
/// `@  : description` (working copy) or `@- : description` (parent). Returns
/// the jj revset (`"@"`, `"@-"`, etc.) suitable for use as a literal revision
/// argument. The summary header is rendered without a change_id, so
/// [`match_commit_header`] (and therefore [`commit_id_at_line`]) doesn't
/// claim it — this helper plugs that gap for code actions and cursor-form
/// resolution that target these summary rows.
pub fn status_summary_header_revset(line: &str) -> Option<String> {
    if !line.starts_with('@') {
        return None;
    }
    let (before_colon, _) = line.split_once(':')?;
    let revset = before_colon.trim();
    if revset == "@" {
        return Some("@".to_string());
    }
    let rest = revset.strip_prefix('@')?;
    if !rest.is_empty() && rest.chars().all(|c| c == '-') {
        return Some(revset.to_string());
    }
    None
}

// --- internal parsers ----------------------------------------------------

/// Graph chars that may appear in the prefix of a `jj log --stat` per-file
/// line. Mirrors the JS `STAT_LINE_RE` character class.
const STAT_GRAPH_CHARS: &[char] = &[
    '│', '○', '●', '◆', '~', '*', '╭', '╮', '╯', '╰', '─', '├', '┤', '┬', '┴', '┼',
];

/// If `line` is a commit-header line (graph prefix + glyph + whitespace +
/// change_id), return the change_id. Otherwise `None`.
///
/// We deliberately don't whitelist the glyph: jj's `templates.log_node` is
/// user-configurable (and even the builtin emits `×` for conflicts), so
/// anything that isn't a graph connector or whitespace is treated as a
/// possible node glyph. The shape of what *follows* the glyph is what
/// confirms the row — 3+ lowercase letters (change_id) ending at whitespace
/// or end-of-line, which the `jj log` template guarantees.
pub(crate) fn match_commit_header(line: &str) -> Option<&str> {
    let mut byte_idx = 0;
    let mut saw_glyph = false;
    for c in line.chars() {
        if is_graph_connector(c) {
            byte_idx += c.len_utf8();
            continue;
        }
        // First non-connector, non-whitespace char is the commit-node glyph
        // for this row. Single character.
        byte_idx += c.len_utf8();
        saw_glyph = true;
        break;
    }
    if !saw_glyph {
        return None;
    }
    let rest = &line[byte_idx..];
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let after_ws = rest.trim_start();
    let end = after_ws
        .find(|c: char| !c.is_ascii_lowercase())
        .unwrap_or(after_ws.len());
    // Need at least 3 lowercase chars for a plausible change_id. Without a
    // glyph whitelist we lean on this shape: a stat-style line like
    // `M src/main.rs` would otherwise parse as glyph=`M`, change_id=`src`.
    if end < 3 {
        return None;
    }
    // The change_id must end at whitespace (real jj log output puts the
    // commit_id after a space) or end-of-line. `src/foo.rs` ending at `/`
    // is what we want to reject here — `/` isn't alphanumeric so the older
    // word-boundary check let it through.
    match after_ws[end..].chars().next() {
        None => Some(&after_ws[..end]),
        Some(c) if c.is_whitespace() => Some(&after_ws[..end]),
        _ => None,
    }
}

/// Graph connector chars (and space) that may appear *before* the commit
/// glyph on an indented row. These are the box-drawing chars `jj` uses to
/// render the DAG (plus `~` for elided revs and the space separator).
fn is_graph_connector(c: char) -> bool {
    matches!(
        c,
        ' ' | '│' | '~' | '╭' | '╮' | '╯' | '╰' | '─' | '├' | '┤' | '┬' | '┴' | '┼'
    )
}

/// Parse a status-section header line: `M src/main.rs`, `A new.txt`, etc.
/// Returns the path (with any rename arrow stripped to the destination).
pub(crate) fn parse_status_header_line(line: &str) -> Option<String> {
    let mut chars = line.chars();
    let flag = chars.next()?;
    if !matches!(flag, 'M' | 'A' | 'D' | 'C' | 'R') {
        return None;
    }
    let rest = &line[flag.len_utf8()..];
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    let path = rest.trim();
    if path.is_empty() {
        return None;
    }
    Some(strip_rename_arrow(path))
}

/// Parse a stat-line: `│  src/main.rs | 3 +++` and similar. Returns the path
/// (with any rename arrow stripped). Returns `None` for the summary line
/// (`N files changed, ...`) and any line lacking the trailing ` | N <+/->`.
fn parse_stat_line(line: &str) -> Option<String> {
    if is_stat_summary_line(line) {
        return None;
    }
    let without_suffix = strip_stat_suffix(line)?;
    let path = strip_stat_prefix(without_suffix).trim();
    if path.is_empty() {
        return None;
    }
    Some(strip_rename_arrow(path))
}

fn parse_file_line(line: &str) -> Option<String> {
    if line.is_empty() {
        return None;
    }
    if let Some(p) = parse_status_header_line(line) {
        return Some(p);
    }
    parse_stat_line(line)
}

/// Match `^\s*N files? changed`. Used to skip the stat summary line which
/// otherwise looks superficially path-like.
fn is_stat_summary_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let digits_end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    if digits_end == 0 {
        return false;
    }
    let after_digits = &trimmed[digits_end..];
    let after_ws = after_digits.trim_start_matches([' ', '\t']);
    if after_ws.len() == after_digits.len() {
        return false;
    }
    after_ws.starts_with("file changed") || after_ws.starts_with("files changed")
}

/// Strip the trailing ` | N <+/->` suffix from a stat line. Returns the
/// portion before that suffix, or `None` if the line lacks it.
///
/// `jj log --stat` right-justifies the change-count column across a group of
/// files, so smaller counts get leading whitespace after the pipe (e.g.
/// `" |  4 +++"` next to `" | 41 ++++..."`). Skip that padding before
/// scanning for digits.
fn strip_stat_suffix(line: &str) -> Option<&str> {
    let bar = line.rfind(" | ")?;
    let tail = &line[bar + 3..];
    let after_pad = tail.trim_start_matches([' ', '\t']);
    let digits_end = after_pad
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(after_pad.len());
    if digits_end == 0 {
        return None;
    }
    let after_digits = &after_pad[digits_end..];
    let after_ws = after_digits.trim_start_matches([' ', '\t']);
    if after_ws.len() == after_digits.len() {
        return None;
    }
    let trimmed_marks = after_ws.trim_end();
    if trimmed_marks.is_empty() {
        return None;
    }
    if !trimmed_marks.chars().all(|c| c == '+' || c == '-') {
        return None;
    }
    Some(&line[..bar])
}

/// Strip leading whitespace and graph chars from a stat-line prefix.
fn strip_stat_prefix(s: &str) -> &str {
    let mut s = s;
    loop {
        let before = s;
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix(|c: char| c == '~' || c == '*') {
            s = rest;
        }
        for &ch in STAT_GRAPH_CHARS {
            if let Some(rest) = s.strip_prefix(ch) {
                s = rest;
            }
        }
        if s.len() == before.len() {
            return s;
        }
    }
}

/// jj renders renames/copies as `old => new`; the destination path is what
/// callers want. Splits on the LAST ` => ` so chained renames still resolve
/// to the final destination.
fn strip_rename_arrow(path: &str) -> String {
    match path.rfind(" => ") {
        Some(idx) => path[idx + 4..].trim().to_string(),
        None => path.to_string(),
    }
}

/// Parse the `PARENT CHANGES (<id>):` section header line and return the
/// change-id inside the parentheses.
fn parse_parent_changes_header(line: &str) -> Option<&str> {
    let after = line.strip_prefix("PARENT CHANGES (")?;
    let end = after.find("):")?;
    let id = &after[..end];
    if id.is_empty() { None } else { Some(id) }
}

/// Return `true` when `lines[line]` falls inside a WORKING COPY CHANGES or
/// PARENT CHANGES block. Walks upward looking for a section header.
fn is_in_changes_section(lines: &[&str], line: usize) -> bool {
    let start = line.min(lines.len().saturating_sub(1));
    for i in (0..=start).rev() {
        let text = lines[i];
        if text.starts_with("WORKING COPY CHANGES (") || text.starts_with("PARENT CHANGES (") {
            return true;
        }
        if text.starts_with("STACK:")
            || text.starts_with("COMMAND REFERENCE:")
            || match_commit_header(text).is_some()
        {
            return false;
        }
    }
    false
}

/// Parse a plain flush-left file path from a CHANGES section line.
/// Rejects blank lines, lines starting with whitespace, section headers,
/// commit-header characters, and diff hunk markers.
fn parse_changes_file_line(line: &str) -> Option<String> {
    if line.is_empty() {
        return None;
    }
    if line.starts_with([' ', '\t']) {
        return None;
    }
    if line.starts_with("WORKING COPY CHANGES")
        || line.starts_with("PARENT CHANGES")
        || line.starts_with("STACK:")
        || line.starts_with("COMMAND REFERENCE:")
        || line.starts_with("MESSAGE:")
        || line.starts_with("Preparing to squash")
        || line.starts_with("@  ")
        || line.starts_with("@- ")
    {
        return None;
    }
    if match_commit_header(line).is_some() {
        return None;
    }
    if is_diff_hunk_line(line) {
        return None;
    }
    Some(line.trim_end().to_string())
}

/// Return `true` for lines that look like diff hunk markers (`@@`) or
/// unified-diff content lines (`+` / `-`).
fn is_diff_hunk_line(line: &str) -> bool {
    line.starts_with("@@") || line.starts_with('+') || line.starts_with('-')
}

/// Parse a `JJ: <Label>: <revset>` log-shortcut line. Mirrors
/// `LOG_SHORTCUT_LINE_RE = /^JJ:\s+([A-Za-z][\w ]*?):\s+(.+)$/`.
fn parse_log_shortcut(line: &str) -> Option<LogShortcut> {
    let after_jj = line.strip_prefix("JJ:")?;
    let after_ws = after_jj.trim_start_matches([' ', '\t']);
    if after_ws.len() == after_jj.len() {
        return None;
    }
    let colon_idx = after_ws.find(':')?;
    let label = &after_ws[..colon_idx];
    let mut label_chars = label.chars();
    let first = label_chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    for c in label_chars {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == ' ') {
            return None;
        }
    }
    let after_colon = &after_ws[colon_idx + 1..];
    let revset_part = after_colon.trim_start_matches([' ', '\t']);
    if revset_part.len() == after_colon.len() {
        return None;
    }
    let revset = revset_part.trim_end();
    if revset.is_empty() {
        return None;
    }
    Some(LogShortcut {
        label: label.to_string(),
        revset: revset.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- BufferKind ---

    #[test]
    fn buffer_kind_detects_each_filename() {
        assert_eq!(
            BufferKind::from_uri("file:///x/.jj/badjuju/status.jujutsu"),
            Some(BufferKind::Status)
        );
        assert_eq!(
            BufferKind::from_uri("file:///x/log.jujutsu"),
            Some(BufferKind::Log)
        );
        // Legacy diff.jujutsu
        assert_eq!(
            BufferKind::from_uri("file:///x/diff.jujutsu"),
            Some(BufferKind::Diff)
        );
        // New per-revision filenames
        assert_eq!(
            BufferKind::from_uri("file:///x/.jj/badjuju/diff-change-abc123def456.jujutsu"),
            Some(BufferKind::Diff)
        );
        assert_eq!(
            BufferKind::from_uri("file:///x/.jj/badjuju/diff-commit-0123456789ab.jujutsu"),
            Some(BufferKind::Diff)
        );
        assert_eq!(BufferKind::from_uri("file:///x/other.txt"), None);
        // Should not match partial names
        assert_eq!(BufferKind::from_uri("file:///x/diff-change-.jujutsu"), None);
        assert_eq!(BufferKind::from_uri("file:///x/diff-commit-.jujutsu"), None);
    }

    #[test]
    fn buffer_kind_hunk_edit_from_uri() {
        assert_eq!(
            BufferKind::from_uri("file:///x/.jj/badjuju/hunk-edit.jujutsu"),
            Some(BufferKind::HunkEdit)
        );
        assert!(is_hunk_edit_uri("file:///x/.jj/badjuju/hunk-edit.jujutsu"));
        assert!(!is_hunk_edit_uri("file:///x/hunk-edit.jujutsu"));
    }

    #[test]
    fn buffer_kind_squash_does_not_claim_hunk_edit_jujutsu() {
        // The squash detector requires a `/squash/` segment in the path, so the
        // top-level hunk-edit.jujutsu must not be misclassified as a squash
        // window even before the explicit hunk-edit check runs.
        assert_ne!(
            BufferKind::from_uri("file:///x/.jj/badjuju/hunk-edit.jujutsu"),
            Some(BufferKind::Squash)
        );
    }

    // --- revision_at_line: Status buffer ---

    fn status_buffer() -> String {
        // Mirrors the shape written by commands::write_status after bad-juju-zgnt:
        //   @  : / @- : header lines, blank line, STACK: line, log output.
        [
            "@  : my working copy change",
            "@- : (empty)",
            "",
            "STACK: ancestors(reachable(@, mutable()), 2)",
            "",
            "@  qpvuntsm 1234abcd",
            "│  description here",
            "○  abcdwxyz e6f7e6f7",
            "│  another commit",
            "◆  zzzzzzzz 0000",
        ]
        .join("\n")
    }

    #[test]
    fn revision_at_line_status_returns_at_on_stat_file_line() {
        // Stat-format file lines in the STACK section belong to the commit
        // whose header they appear under.
        let s = [
            "@  : my change",
            "@- : parent",
            "",
            "STACK: @",
            "",
            "@  qpvuntsm 1234abcd",
            "│  M src/main.rs",
        ]
        .join("\n");
        // Line 6 = "│  M src/main.rs" → parse_status_header_line returns None
        // (graph-prefix line), so walk up hits qpvuntsm at line 5.
        // BUT line 6 is a file-stat line under the commit: revision is qpvuntsm.
        // parse_status_header_line looks for "M …" without graph prefix;
        // "│  M src/main.rs" has a graph prefix so it's handled by the walk.
        assert_eq!(
            revision_at_line(&s, 6, BufferKind::Status).as_deref(),
            Some("qpvuntsm")
        );
    }

    #[test]
    fn revision_at_line_status_on_parent_summary_header_returns_at_minus() {
        let s = status_buffer();
        // Line 1 = "@- : (empty)" — the summary parent header line.
        // status_summary_header_revset claims it directly, so the revision
        // is `@-` (not the @ default).
        assert_eq!(
            revision_at_line(&s, 1, BufferKind::Status).as_deref(),
            Some("@-")
        );
    }

    #[test]
    fn revision_at_line_status_on_at_summary_header_returns_at() {
        let s = status_buffer();
        // Line 0 = "@  : my working copy change" — the summary @ header line.
        assert_eq!(
            revision_at_line(&s, 0, BufferKind::Status).as_deref(),
            Some("@")
        );
    }

    #[test]
    fn revision_at_line_status_returns_change_id_on_commit_header() {
        let s = status_buffer();
        // Line 5 = "@  qpvuntsm 1234abcd"
        assert_eq!(
            revision_at_line(&s, 5, BufferKind::Status).as_deref(),
            Some("qpvuntsm")
        );
        // Line 7 = "○  abcdwxyz e6f7e6f7"
        assert_eq!(
            revision_at_line(&s, 7, BufferKind::Status).as_deref(),
            Some("abcdwxyz")
        );
    }

    #[test]
    fn revision_at_line_status_walks_up_to_nearest_commit() {
        let s = status_buffer();
        // Line 8 = "│  another commit" — should walk up to abcdwxyz.
        assert_eq!(
            revision_at_line(&s, 8, BufferKind::Status).as_deref(),
            Some("abcdwxyz")
        );
    }

    #[test]
    fn revision_at_line_status_off_the_end_returns_at_or_last_walk() {
        let s = status_buffer();
        // Walking up from out-of-bounds clamps to last line, which is
        // "◆  zzzzzzzz 0000" — a commit header. Result is its change_id.
        assert_eq!(
            revision_at_line(&s, 9999, BufferKind::Status).as_deref(),
            Some("zzzzzzzz")
        );
    }

    #[test]
    fn revision_at_line_status_empty_input_returns_at() {
        assert_eq!(
            revision_at_line("", 0, BufferKind::Status).as_deref(),
            Some("@")
        );
    }

    // --- revision_at_line: Log buffer ---

    fn log_buffer() -> String {
        [
            "REVSET: @",
            "JJ: Mutable: ancestors(reachable(@, mutable()))",
            "JJ: Stack:   (immutable_heads()..@)::",
            "",
            "OUTPUT:",
            "",
            "@  qpvuntsm 1234abcd",
            "│  description here",
            "○  abcdwxyz e6f7e6f7",
        ]
        .join("\n")
    }

    #[test]
    fn revision_at_line_log_returns_change_id_on_commit_header() {
        let s = log_buffer();
        assert_eq!(
            revision_at_line(&s, 6, BufferKind::Log).as_deref(),
            Some("qpvuntsm")
        );
    }

    #[test]
    fn revision_at_line_log_walks_up_to_nearest_commit() {
        let s = log_buffer();
        // Line 7 = "│  description here" — walks up to qpvuntsm.
        assert_eq!(
            revision_at_line(&s, 7, BufferKind::Log).as_deref(),
            Some("qpvuntsm")
        );
    }

    #[test]
    fn revision_at_line_log_returns_none_above_any_commit() {
        let s = log_buffer();
        // Line 2 = a shortcut line in the header — no commit above.
        assert_eq!(revision_at_line(&s, 2, BufferKind::Log), None);
    }

    #[test]
    fn revision_at_line_log_empty_input_returns_none() {
        assert_eq!(revision_at_line("", 0, BufferKind::Log), None);
    }

    #[test]
    fn revision_at_line_log_off_the_end_clamps_to_last_line() {
        let s = log_buffer();
        // Out-of-bounds clamps to the last line, which is a commit header.
        assert_eq!(
            revision_at_line(&s, 9999, BufferKind::Log).as_deref(),
            Some("abcdwxyz")
        );
    }

    // --- revision_at_line: Diff buffer ---

    #[test]
    fn revision_at_line_diff_legacy_revision_header() {
        let content = "REVISION: abc123\n\nDIFF:\n\n@@ -1 +1 @@\n-foo\n+bar\n";
        assert_eq!(
            revision_at_line(content, 5, BufferKind::Diff).as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn revision_at_line_diff_change_id_header() {
        let content = "CHANGE_ID: kkmpptxzrspxwutvuoqnzoqmsqrqkxpt\n\nDIFF:\n\n";
        assert_eq!(
            revision_at_line(content, 2, BufferKind::Diff).as_deref(),
            Some("kkmpptxzrspxwutvuoqnzoqmsqrqkxpt")
        );
    }

    #[test]
    fn revision_at_line_diff_commit_id_header() {
        let content = "COMMIT_ID: 0102030405060708090a0b0c0d0e0f10\n\nDIFF:\n";
        assert_eq!(
            revision_at_line(content, 1, BufferKind::Diff).as_deref(),
            Some("0102030405060708090a0b0c0d0e0f10")
        );
    }

    #[test]
    fn revision_at_line_diff_missing_header_returns_none() {
        assert_eq!(
            revision_at_line("no header here\n", 0, BufferKind::Diff),
            None
        );
    }

    #[test]
    fn revision_at_line_diff_blank_header_returns_none() {
        assert_eq!(
            revision_at_line("REVISION:   \n\nDIFF:\n", 2, BufferKind::Diff),
            None
        );
        assert_eq!(
            revision_at_line("CHANGE_ID:   \n\nDIFF:\n", 2, BufferKind::Diff),
            None
        );
    }

    // --- file_at_line ---

    #[test]
    fn file_at_line_parses_status_header_lines() {
        let content = "M src/main.rs\nA new.txt\nD gone.txt\nC copied.txt\nR renamed.txt";
        assert_eq!(file_at_line(content, 0).as_deref(), Some("src/main.rs"));
        assert_eq!(file_at_line(content, 1).as_deref(), Some("new.txt"));
        assert_eq!(file_at_line(content, 2).as_deref(), Some("gone.txt"));
        assert_eq!(file_at_line(content, 3).as_deref(), Some("copied.txt"));
        assert_eq!(file_at_line(content, 4).as_deref(), Some("renamed.txt"));
    }

    #[test]
    fn file_at_line_handles_paths_with_spaces() {
        let content = "M a b/c d.txt";
        assert_eq!(file_at_line(content, 0).as_deref(), Some("a b/c d.txt"));
    }

    #[test]
    fn file_at_line_returns_rename_destination() {
        let content = "R old/path.rs => new/path.rs";
        assert_eq!(file_at_line(content, 0).as_deref(), Some("new/path.rs"));
    }

    #[test]
    fn file_at_line_parses_stat_lines() {
        let content = "│  src/main.rs | 3 +++\n├─╮  foo.rs | 1 +";
        assert_eq!(file_at_line(content, 0).as_deref(), Some("src/main.rs"));
        assert_eq!(file_at_line(content, 1).as_deref(), Some("foo.rs"));
    }

    #[test]
    fn file_at_line_stat_summary_returns_none() {
        let content = "5 files changed, 30 insertions(+), 2 deletions(-)";
        assert_eq!(file_at_line(content, 0), None);
        let one = "1 file changed, 3 insertions(+)";
        assert_eq!(file_at_line(one, 0), None);
    }

    #[test]
    fn file_at_line_unparseable_returns_none() {
        let content = "Working copy changes:\n@  qpvuntsm 1234abcd\n";
        assert_eq!(file_at_line(content, 0), None);
        assert_eq!(file_at_line(content, 1), None);
    }

    #[test]
    fn file_at_line_blank_line_returns_none() {
        assert_eq!(file_at_line("\n\n", 0), None);
    }

    #[test]
    fn file_at_line_off_the_end_returns_none() {
        assert_eq!(file_at_line("M foo.rs", 99), None);
    }

    #[test]
    fn file_at_line_stat_line_with_rename_returns_destination() {
        let content = "│  old.rs => new.rs | 2 +-";
        assert_eq!(file_at_line(content, 0).as_deref(), Some("new.rs"));
    }

    // --- log_shortcut_at_line ---

    #[test]
    fn log_shortcut_at_line_parses_simple() {
        let content = "JJ: Mutable: ancestors(reachable(@, mutable()))";
        assert_eq!(
            log_shortcut_at_line(content, 0),
            Some(LogShortcut {
                label: "Mutable".to_string(),
                revset: "ancestors(reachable(@, mutable()))".to_string(),
            })
        );
    }

    #[test]
    fn log_shortcut_at_line_parses_aligned_form() {
        // The rendered form has multiple spaces after the colon for alignment.
        let content = "JJ: Stack:   (immutable_heads()..@)::";
        assert_eq!(
            log_shortcut_at_line(content, 0),
            Some(LogShortcut {
                label: "Stack".to_string(),
                revset: "(immutable_heads()..@)::".to_string(),
            })
        );
    }

    #[test]
    fn log_shortcut_at_line_allows_spaces_in_label() {
        let content = "JJ: Working Copy: @";
        assert_eq!(
            log_shortcut_at_line(content, 0),
            Some(LogShortcut {
                label: "Working Copy".to_string(),
                revset: "@".to_string(),
            })
        );
    }

    #[test]
    fn log_shortcut_at_line_non_shortcut_returns_none() {
        assert_eq!(log_shortcut_at_line("REVSET: @", 0), None);
        assert_eq!(log_shortcut_at_line("@  qpvuntsm 1234", 0), None);
        assert_eq!(log_shortcut_at_line("", 0), None);
    }

    #[test]
    fn log_shortcut_at_line_off_the_end_returns_none() {
        assert_eq!(log_shortcut_at_line("JJ: Foo: bar", 99), None);
    }

    #[test]
    fn log_shortcut_at_line_rejects_label_starting_with_digit() {
        assert_eq!(log_shortcut_at_line("JJ: 1foo: bar", 0), None);
    }

    #[test]
    fn log_shortcut_at_line_rejects_missing_revset() {
        assert_eq!(log_shortcut_at_line("JJ: Foo:", 0), None);
        assert_eq!(log_shortcut_at_line("JJ: Foo:   ", 0), None);
    }

    // --- CHANGES section: revision_at_line ---

    fn status_with_changes() -> String {
        [
            "@  : my working copy change",
            "@- : parent",
            "",
            "WORKING COPY CHANGES (yyzmyynq):",
            "foo.txt",
            "bar.txt",
            "",
            "PARENT CHANGES (uqzpovpt):",
            "baz.txt",
            "",
            "STACK: ancestors(reachable(@, mutable()), 2)",
            "",
            "@  qpvuntsm 1234abcd",
            "│  description here",
        ]
        .join("\n")
    }

    #[test]
    fn revision_at_line_working_copy_section_returns_at() {
        let s = status_with_changes();
        // Line 3 = section header
        assert_eq!(
            revision_at_line(&s, 3, BufferKind::Status).as_deref(),
            Some("@")
        );
        // Line 4 = foo.txt
        assert_eq!(
            revision_at_line(&s, 4, BufferKind::Status).as_deref(),
            Some("@")
        );
        // Line 5 = bar.txt
        assert_eq!(
            revision_at_line(&s, 5, BufferKind::Status).as_deref(),
            Some("@")
        );
    }

    #[test]
    fn revision_at_line_parent_changes_section_returns_parent_id() {
        let s = status_with_changes();
        // Line 7 = PARENT CHANGES header
        assert_eq!(
            revision_at_line(&s, 7, BufferKind::Status).as_deref(),
            Some("uqzpovpt")
        );
        // Line 8 = baz.txt
        assert_eq!(
            revision_at_line(&s, 8, BufferKind::Status).as_deref(),
            Some("uqzpovpt")
        );
    }

    #[test]
    fn revision_at_line_merge_two_parent_changes_each_returns_correct_id() {
        let s = [
            "@  : merge commit",
            "@- : branch-a",
            "@- : branch-b",
            "",
            "PARENT CHANGES (aaaabbbb):",
            "from-a.txt",
            "",
            "PARENT CHANGES (ccccdddd):",
            "from-b.txt",
            "",
            "STACK: @",
        ]
        .join("\n");
        assert_eq!(
            revision_at_line(&s, 5, BufferKind::Status).as_deref(),
            Some("aaaabbbb")
        );
        assert_eq!(
            revision_at_line(&s, 8, BufferKind::Status).as_deref(),
            Some("ccccdddd")
        );
    }

    #[test]
    fn revision_at_line_stack_section_unchanged() {
        let s = status_with_changes();
        // Line 12 = @  qpvuntsm (commit header in STACK)
        assert_eq!(
            revision_at_line(&s, 12, BufferKind::Status).as_deref(),
            Some("qpvuntsm")
        );
        // Line 13 = "│  description here" — walks up to qpvuntsm
        assert_eq!(
            revision_at_line(&s, 13, BufferKind::Status).as_deref(),
            Some("qpvuntsm")
        );
    }

    // --- CHANGES section: file_at_line ---

    #[test]
    fn file_at_line_plain_path_in_working_copy_changes() {
        let s = status_with_changes();
        // Line 4 = "foo.txt"
        assert_eq!(file_at_line(&s, 4).as_deref(), Some("foo.txt"));
        // Line 5 = "bar.txt"
        assert_eq!(file_at_line(&s, 5).as_deref(), Some("bar.txt"));
    }

    #[test]
    fn file_at_line_plain_path_in_parent_changes() {
        let s = status_with_changes();
        // Line 8 = "baz.txt"
        assert_eq!(file_at_line(&s, 8).as_deref(), Some("baz.txt"));
    }

    #[test]
    fn file_at_line_section_header_returns_none() {
        let s = status_with_changes();
        // Line 3 = "WORKING COPY CHANGES (yyzmyynq):"
        assert_eq!(file_at_line(&s, 3), None);
        // Line 7 = "PARENT CHANGES (uqzpovpt):"
        assert_eq!(file_at_line(&s, 7), None);
    }

    #[test]
    fn file_at_line_plain_path_outside_changes_section_returns_none() {
        let s = status_with_changes();
        // Line 0 = "@  : my working copy change" — not in a CHANGES section
        assert_eq!(file_at_line(&s, 0), None);
        // Line 10 = "STACK: ..." — not in a CHANGES section
        assert_eq!(file_at_line(&s, 10), None);
    }

    #[test]
    fn file_at_line_diff_hunk_walks_up_to_enclosing_file() {
        let s = [
            "@  : my change",
            "@- : parent",
            "",
            "WORKING COPY CHANGES (yyzmyynq):",
            "readme.txt",
            "@@ -1,2 +1,3 @@",
            " unchanged",
            "+new line",
            "-old line",
            "",
            "STACK: @",
        ]
        .join("\n");
        // @@ hunk header on line 5 → walks up to readme.txt on line 4
        assert_eq!(file_at_line(&s, 5).as_deref(), Some("readme.txt"));
        // Diff context/add/remove lines also walk up — including space-prefixed
        // context lines (the new-side line number maps to the enclosing file).
        assert_eq!(file_at_line(&s, 6).as_deref(), Some("readme.txt")); // " unchanged" context
        assert_eq!(file_at_line(&s, 7).as_deref(), Some("readme.txt")); // +new line
        assert_eq!(file_at_line(&s, 8).as_deref(), Some("readme.txt")); // -old line
    }

    // --- cursor_target_at_line ---

    #[test]
    fn cursor_target_working_copy_file() {
        let s = status_with_changes();
        assert_eq!(
            cursor_target_at_line(&s, 4),
            Some(CursorTarget::WorkingCopyFile {
                path: "foo.txt".to_string()
            })
        );
    }

    #[test]
    fn cursor_target_parent_file() {
        let s = status_with_changes();
        assert_eq!(
            cursor_target_at_line(&s, 8),
            Some(CursorTarget::ParentFile {
                parent_id: "uqzpovpt".to_string(),
                path: "baz.txt".to_string()
            })
        );
    }

    #[test]
    fn cursor_target_merge_second_parent() {
        let s = [
            "@  : merge",
            "@- : branch-a",
            "@- : branch-b",
            "",
            "PARENT CHANGES (aaaabbbb):",
            "from-a.txt",
            "",
            "PARENT CHANGES (ccccdddd):",
            "from-b.txt",
            "",
            "STACK: @",
        ]
        .join("\n");
        assert_eq!(
            cursor_target_at_line(&s, 8),
            Some(CursorTarget::ParentFile {
                parent_id: "ccccdddd".to_string(),
                path: "from-b.txt".to_string()
            })
        );
    }

    #[test]
    fn cursor_target_stack_commit_file() {
        let s = [
            "@  : my change",
            "@- : parent",
            "",
            "STACK: @",
            "",
            "@  qpvuntsm 1234abcd",
            "│  description here",
            "│  src/main.rs | 3 +++",
        ]
        .join("\n");
        assert_eq!(
            cursor_target_at_line(&s, 7),
            Some(CursorTarget::StackCommitFile {
                change_id: "qpvuntsm".to_string(),
                path: "src/main.rs".to_string()
            })
        );
    }

    #[test]
    fn cursor_target_non_file_line_returns_none() {
        let s = status_with_changes();
        // Section header line
        assert_eq!(cursor_target_at_line(&s, 3), None);
        // Blank line
        assert_eq!(cursor_target_at_line(&s, 2), None);
        // @ header line
        assert_eq!(cursor_target_at_line(&s, 0), None);
    }

    // --- commit_id_at_line ---

    #[test]
    fn commit_id_at_line_returns_id_on_header_line() {
        let content = "@  qpvuntsm 1234abcd\n│  description\n○  abcdwxyz e6f7\n";
        assert_eq!(commit_id_at_line(content, 0).as_deref(), Some("qpvuntsm"));
        assert_eq!(commit_id_at_line(content, 2).as_deref(), Some("abcdwxyz"));
    }

    #[test]
    fn commit_id_at_line_returns_none_for_description_line() {
        // Even though revision_at_line walks up to find the commit, the strict
        // variant returns None on a description line.
        let content = "@  qpvuntsm 1234abcd\n│  description\n";
        assert_eq!(commit_id_at_line(content, 1), None);
    }

    #[test]
    fn commit_id_at_line_returns_none_for_status_file_line() {
        let content = "M src/main.rs\nA new.txt\n";
        assert_eq!(commit_id_at_line(content, 0), None);
        assert_eq!(commit_id_at_line(content, 1), None);
    }

    #[test]
    fn commit_id_at_line_returns_none_off_the_end() {
        assert_eq!(commit_id_at_line("@  qpvuntsm 1234", 99), None);
    }

    #[test]
    fn commit_id_at_line_returns_none_on_empty() {
        assert_eq!(commit_id_at_line("", 0), None);
    }

    #[test]
    fn commit_id_at_line_returns_id_on_indented_branch_header() {
        // Branched log output puts a commit one or more graph cells in from
        // the left: `│ ○  qqrpmryt …`. The commit char isn't at column 0 but
        // it's still a commit-header row.
        let content = "@  vzkvvnwk 3cdaebd6\n○  nslnmquu 774297e0\n│ ○  qqrpmryt 1354dff3\n";
        assert_eq!(commit_id_at_line(content, 2).as_deref(), Some("qqrpmryt"));
    }

    #[test]
    fn commit_id_at_line_rejects_stat_line_with_graph_prefix() {
        // Stat lines also start with graph chars but have no commit glyph in
        // the prefix — they must not be misidentified as commit headers.
        let content = "○  abcdefgh first\n│  src/main.rs | 3 +++\n├─╯  other.rs | 1 +\n";
        assert_eq!(commit_id_at_line(content, 1), None);
        assert_eq!(commit_id_at_line(content, 2), None);
    }

    #[test]
    fn commit_id_at_line_recognizes_custom_node_glyphs() {
        // jj's `templates.log_node` is user-configurable, and even the builtin
        // emits `×` for conflicts. We must not hardcode a glyph allowlist —
        // any single non-connector char before `<ws><change_id>` is the glyph.
        for glyph in ['×', '◌', '✗', 'W'] {
            let line = format!("{glyph}  qpvuntsm 1234abcd description");
            assert_eq!(
                commit_id_at_line(&line, 0).as_deref(),
                Some("qpvuntsm"),
                "expected to recognize {glyph:?} as a commit-node glyph in {line:?}"
            );
        }
    }

    #[test]
    fn commit_id_at_line_rejects_m_flag_status_line() {
        // `M src/main.rs` looks superficially like `<glyph><ws><change_id>`
        // (glyph=`M`, then `src`) — and would silently misparse if the
        // trailing boundary were just "non-alphanumeric". The change_id has
        // to end at whitespace (or EOL), not at `/`.
        assert_eq!(commit_id_at_line("M src/main.rs", 0), None);
        assert_eq!(commit_id_at_line("A new.txt", 0), None);
        assert_eq!(commit_id_at_line("D gone.rs", 0), None);
    }

    #[test]
    fn commit_id_at_line_rejects_short_change_id_under_three_chars() {
        // Without a glyph whitelist we use the change_id's shape as the
        // confirming signal. Fewer than 3 lowercase letters is too generic
        // (any `M x` style line would pass).
        assert_eq!(commit_id_at_line("○  ab 1234abcd desc", 0), None);
        // 3 is the minimum that counts.
        assert_eq!(
            commit_id_at_line("○  abc 1234abcd desc", 0).as_deref(),
            Some("abc")
        );
    }

    // --- status_summary_header_revset ---

    #[test]
    fn status_summary_header_revset_matches_at_working_copy_line() {
        assert_eq!(
            status_summary_header_revset("@  : (empty) my work").as_deref(),
            Some("@")
        );
    }

    #[test]
    fn status_summary_header_revset_matches_at_parent_line() {
        assert_eq!(
            status_summary_header_revset("@- : (empty) parent").as_deref(),
            Some("@-")
        );
    }

    #[test]
    fn status_summary_header_revset_matches_grandparent_line() {
        assert_eq!(
            status_summary_header_revset("@-- : grandparent").as_deref(),
            Some("@--")
        );
    }

    #[test]
    fn status_summary_header_revset_with_bookmarks() {
        assert_eq!(
            status_summary_header_revset("@- : <main@origin> parent desc").as_deref(),
            Some("@-")
        );
    }

    #[test]
    fn status_summary_header_revset_rejects_non_at_lines() {
        assert_eq!(status_summary_header_revset("M src/main.rs"), None);
        assert_eq!(status_summary_header_revset("STACK: @"), None);
        assert_eq!(status_summary_header_revset(""), None);
        assert_eq!(status_summary_header_revset("@  qpvuntsm 1234abcd"), None);
    }

    // --- parse_hunk_header_new_start ---

    #[test]
    fn parse_hunk_header_new_start_two_part_form() {
        assert_eq!(parse_hunk_header_new_start("@@ -1,3 +5,2 @@"), Some(5));
    }

    #[test]
    fn parse_hunk_header_new_start_single_count_form() {
        assert_eq!(parse_hunk_header_new_start("@@ -1 +7 @@"), Some(7));
    }

    #[test]
    fn parse_hunk_header_new_start_with_trailing_context() {
        assert_eq!(
            parse_hunk_header_new_start("@@ -1,3 +5,2 @@ fn foo()"),
            Some(5)
        );
    }

    #[test]
    fn parse_hunk_header_new_start_rejects_non_hunk_header() {
        assert_eq!(parse_hunk_header_new_start("+ added"), None);
        assert_eq!(parse_hunk_header_new_start("+++ b/foo"), None);
        assert_eq!(parse_hunk_header_new_start(""), None);
    }

    // --- file_target_at_line: Status ---

    fn status_with_diff_hunks() -> String {
        // CHANGES section with embedded unified-diff hunks under bare filename rows.
        [
            "@  : my change",
            "@- : parent",
            "",
            "WORKING COPY CHANGES (yyzmyynq):",
            "src/main.rs",
            "@@ -1,3 +5,3 @@",
            " ctx-a",
            "-removed",
            "+added",
            "",
            "src/other.rs",
            "@@ -10,1 +20,2 @@",
            " keep",
            "+inserted",
            "",
            "STACK: @",
        ]
        .join("\n")
    }

    #[test]
    fn file_target_status_on_filename_row() {
        let s = status_with_diff_hunks();
        // Line 4 = "src/main.rs"
        let t = file_target_at_line(&s, 4, BufferKind::Status).unwrap();
        assert_eq!(t.path, "src/main.rs");
        assert_eq!(t.revision, "@");
        assert_eq!(t.line_in_file, None);
        assert!(!t.on_minus_line);
    }

    #[test]
    fn file_target_status_on_hunk_header_row() {
        let s = status_with_diff_hunks();
        // Line 5 = "@@ -1,3 +5,3 @@"
        let t = file_target_at_line(&s, 5, BufferKind::Status).unwrap();
        assert_eq!(t.path, "src/main.rs");
        assert_eq!(t.revision, "@");
        assert_eq!(t.line_in_file, Some(5));
        assert!(!t.on_minus_line);
    }

    #[test]
    fn file_target_status_on_plus_minus_space_rows() {
        let s = status_with_diff_hunks();
        // Line 6 = " ctx-a" (context, counts as 1 from header)
        let t = file_target_at_line(&s, 6, BufferKind::Status).unwrap();
        assert_eq!(t.line_in_file, Some(5));
        assert!(!t.on_minus_line);
        // Line 7 = "-removed" (no +/space above in hunk → count = 1; new_start + 1 = 6)
        let t = file_target_at_line(&s, 7, BufferKind::Status).unwrap();
        assert_eq!(t.line_in_file, Some(6));
        assert!(t.on_minus_line);
        // Line 8 = "+added" — count of +/space inclusive: line 6 (' '), line 8 ('+') = 2.
        // new_start + 2 - 1 = 6.
        let t = file_target_at_line(&s, 8, BufferKind::Status).unwrap();
        assert_eq!(t.line_in_file, Some(6));
        assert!(!t.on_minus_line);
    }

    // --- file_target_at_line: Log ---

    #[test]
    fn file_target_log_on_stat_file_row() {
        // Log buffer renders stat lines under commit headers with `--stat`.
        let s = [
            "REVSET: @",
            "",
            "OUTPUT:",
            "",
            "@  qpvuntsm 1234abcd hello (now me@x.com)",
            "│  src/main.rs | 3 +++",
        ]
        .join("\n");
        let t = file_target_at_line(&s, 5, BufferKind::Log).unwrap();
        assert_eq!(t.path, "src/main.rs");
        assert_eq!(t.revision, "qpvuntsm");
        assert_eq!(t.line_in_file, None);
        assert!(!t.on_minus_line);
    }

    #[test]
    fn file_target_log_on_hunk_header_row() {
        // Log buffers don't normally embed @@ hunks, but if they do (e.g.
        // a future `--patch` mode), the function should still resolve the
        // line. Use a CHANGES-style synthetic context with a bare filename
        // row above so file_at_line finds the path.
        let s = [
            "REVSET: @",
            "",
            "OUTPUT:",
            "",
            "@  qpvuntsm 1234abcd hello (now me@x.com)",
            "│  src/main.rs | 1 +",
        ]
        .join("\n");
        // Stat-line row is a "file row" with no hunk above: cursor on the
        // stat row itself returns no line_in_file.
        let t = file_target_at_line(&s, 5, BufferKind::Log).unwrap();
        assert_eq!(t.line_in_file, None);
    }

    #[test]
    fn file_target_log_on_non_file_row_returns_none() {
        let s = [
            "REVSET: @",
            "JJ: Mutable: ancestors(mutable())",
            "",
            "OUTPUT:",
            "",
            "@  qpvuntsm 1234abcd hello (now me@x.com)",
        ]
        .join("\n");
        // The header / shortcut / commit-header rows aren't file rows.
        assert_eq!(file_target_at_line(&s, 0, BufferKind::Log), None);
        assert_eq!(file_target_at_line(&s, 1, BufferKind::Log), None);
        assert_eq!(file_target_at_line(&s, 5, BufferKind::Log), None);
    }

    // --- file_target_at_line: Diff ---

    fn diff_buffer() -> String {
        [
            "COMMIT_ID: 0102030405060708090a0b0c0d0e0f10",
            "",
            "DIFF:",
            "",
            "diff --git a/src/main.rs b/src/main.rs",
            "index 1111..2222 100644",
            "--- a/src/main.rs",
            "+++ b/src/main.rs",
            "@@ -1,3 +10,3 @@",
            " ctx",
            "-old",
            "+new",
            "diff --git a/README.md b/README.md",
            "index 3333..4444 100644",
            "--- a/README.md",
            "+++ b/README.md",
            "@@ -5,1 +5,2 @@",
            " keep",
            "+added",
        ]
        .join("\n")
    }

    #[test]
    fn file_target_diff_on_filename_row() {
        let s = diff_buffer();
        // Line 7 = "+++ b/src/main.rs"
        let t = file_target_at_line(&s, 7, BufferKind::Diff).unwrap();
        assert_eq!(t.path, "src/main.rs");
        assert_eq!(t.revision, "0102030405060708090a0b0c0d0e0f10");
        assert_eq!(t.line_in_file, None);
        assert!(!t.on_minus_line);
    }

    #[test]
    fn file_target_diff_on_hunk_header_row() {
        let s = diff_buffer();
        // Line 8 = "@@ -1,3 +10,3 @@"
        let t = file_target_at_line(&s, 8, BufferKind::Diff).unwrap();
        assert_eq!(t.path, "src/main.rs");
        assert_eq!(t.line_in_file, Some(10));
        assert!(!t.on_minus_line);
    }

    #[test]
    fn file_target_diff_on_plus_minus_space_rows() {
        let s = diff_buffer();
        // Line 9 = " ctx" → context counts 1 → new_start + 1 - 1 = 10
        let t = file_target_at_line(&s, 9, BufferKind::Diff).unwrap();
        assert_eq!(t.line_in_file, Some(10));
        assert!(!t.on_minus_line);
        // Line 10 = "-old" → +/space strictly above = 1 (the ctx) → 10 + 1 = 11
        let t = file_target_at_line(&s, 10, BufferKind::Diff).unwrap();
        assert_eq!(t.line_in_file, Some(11));
        assert!(t.on_minus_line);
        // Line 11 = "+new" → +/space inclusive: ctx(1) + new(1) = 2 → 10 + 2 - 1 = 11
        let t = file_target_at_line(&s, 11, BufferKind::Diff).unwrap();
        assert_eq!(t.line_in_file, Some(11));
        assert!(!t.on_minus_line);
    }

    #[test]
    fn file_target_diff_hunk_walk_stops_at_file_boundary() {
        let s = diff_buffer();
        // Line 18 = "+added" — its enclosing hunk is line 16, file is README.md.
        // Walk-up must not escape into src/main.rs's hunks.
        let t = file_target_at_line(&s, 18, BufferKind::Diff).unwrap();
        assert_eq!(t.path, "README.md");
        // Inclusive count: " keep" + "+added" = 2 → new_start(5) + 2 - 1 = 6
        assert_eq!(t.line_in_file, Some(6));
    }

    #[test]
    fn file_target_diff_dev_null_returns_none() {
        let s = [
            "COMMIT_ID: abc",
            "",
            "DIFF:",
            "",
            "diff --git a/gone.txt b/gone.txt",
            "deleted file mode 100644",
            "index 0000..1111",
            "--- a/gone.txt",
            "+++ /dev/null",
            "@@ -1,1 +0,0 @@",
            "-removed",
        ]
        .join("\n");
        // No meaningful path at this revision (file deleted).
        assert_eq!(file_target_at_line(&s, 10, BufferKind::Diff), None);
    }

    // --- file_target_at_line: Squash ---

    fn squash_buffer() -> String {
        [
            "SQUASHING:",
            "From: abcdwxyz 11112222 source",
            "To:   qpvuntsm 33334444 dest",
            "",
            "SELECTED CHANGES:",
            "",
            "REMAINING CHANGES:",
            "src/main.rs",
            "@@ -1,3 +5,3 @@",
            " ctx",
            "-removed",
            "+added",
            "",
            "src/other.rs",
            "@@ -10,1 +20,2 @@",
            " keep",
            "+inserted",
        ]
        .join("\n")
    }

    #[test]
    fn file_target_squash_on_filename_row() {
        let s = squash_buffer();
        // Line 7 = "src/main.rs"
        let t = file_target_at_line(&s, 7, BufferKind::Squash).unwrap();
        assert_eq!(t.path, "src/main.rs");
        assert_eq!(t.revision, "abcdwxyz");
        assert_eq!(t.line_in_file, None);
        assert!(!t.on_minus_line);
    }

    #[test]
    fn file_target_squash_on_hunk_header_row() {
        let s = squash_buffer();
        // Line 8 = "@@ -1,3 +5,3 @@"
        let t = file_target_at_line(&s, 8, BufferKind::Squash).unwrap();
        assert_eq!(t.path, "src/main.rs");
        assert_eq!(t.revision, "abcdwxyz");
        assert_eq!(t.line_in_file, Some(5));
    }

    #[test]
    fn file_target_squash_on_plus_minus_space_rows() {
        let s = squash_buffer();
        // Line 9 = " ctx" — context counts 1 → 5
        let t = file_target_at_line(&s, 9, BufferKind::Squash).unwrap();
        assert_eq!(t.line_in_file, Some(5));
        assert!(!t.on_minus_line);
        // Line 10 = "-removed" → 5 + 1 = 6
        let t = file_target_at_line(&s, 10, BufferKind::Squash).unwrap();
        assert_eq!(t.line_in_file, Some(6));
        assert!(t.on_minus_line);
        // Line 11 = "+added" → 5 + 2 - 1 = 6
        let t = file_target_at_line(&s, 11, BufferKind::Squash).unwrap();
        assert_eq!(t.line_in_file, Some(6));
        assert!(!t.on_minus_line);
    }

    // --- file_target_at_line: misc ---

    #[test]
    fn file_target_blank_line_returns_none() {
        let s = squash_buffer();
        // Line 12 = "" between files.
        assert_eq!(file_target_at_line(&s, 12, BufferKind::Squash), None);
    }

    #[test]
    fn file_target_hunk_edit_returns_none() {
        // HunkEdit buffers are action-only; no file resolution.
        let s = "@@ -1,1 +1,1 @@\n-old\n+new\n";
        assert_eq!(file_target_at_line(s, 0, BufferKind::HunkEdit), None);
    }

    #[test]
    fn file_target_empty_content_returns_none() {
        assert_eq!(file_target_at_line("", 0, BufferKind::Status), None);
    }

    #[test]
    fn file_target_off_the_end_clamps_to_last_line() {
        let s = squash_buffer();
        // Way past end — should clamp; last line is "+inserted" under src/other.rs.
        let t = file_target_at_line(&s, 9999, BufferKind::Squash).unwrap();
        assert_eq!(t.path, "src/other.rs");
        assert_eq!(t.line_in_file, Some(21));
    }

    #[test]
    fn file_target_status_stack_stat_row_with_right_justified_count() {
        // jj log --stat right-justifies the change-count column across the
        // group of files in one commit, so smaller counts get an extra space
        // after the pipe (`|  4 +++` next to `| 41 ++++...`). Regression test
        // for parse_stat_line dropping these rows and gri/gd reporting
        // "No locations found" on them.
        let s = [
            "STACK: ancestors(reachable(@, mutable()), 2)",
            "",
            "○  opwrmymx 2e9e4d4e feat(neovim): ...",
            "│  clients/neovim/lua/badjuju/keymap.lua |  4 +++",
            "│  clients/neovim/tests/keymap_spec.lua  | 41 ++++++",
        ]
        .join("\n");
        let t = file_target_at_line(&s, 3, BufferKind::Status).unwrap();
        assert_eq!(t.path, "clients/neovim/lua/badjuju/keymap.lua");
        assert_eq!(t.revision, "opwrmymx");
        let t = file_target_at_line(&s, 4, BufferKind::Status).unwrap();
        assert_eq!(t.path, "clients/neovim/tests/keymap_spec.lua");
        assert_eq!(t.revision, "opwrmymx");
    }
}
