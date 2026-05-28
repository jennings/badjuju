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
}

impl BufferKind {
    /// Detect kind from a URI string by trailing filename. Returns `None` for
    /// URIs that don't end in one of the known buffer names.
    pub fn from_uri(uri: &str) -> Option<Self> {
        if uri.ends_with("status.jujutsu") {
            Some(Self::Status)
        } else if uri.ends_with("log.jujutsu") {
            Some(Self::Log)
        } else if is_diff_uri(uri) {
            Some(Self::Diff)
        } else {
            None
        }
    }
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
    }
}

fn revision_at_line_status(content: &str, line: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let current = lines.get(line).copied().unwrap_or("");
    if parse_status_header_line(current).is_some() {
        return "@".to_string();
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
    if is_diff_hunk_line(line_text) {
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

// --- internal parsers ----------------------------------------------------

const COMMIT_HEADER_CHARS: &[char] = &['@', '*', '○', '●', '◆'];

/// Graph chars that may appear in the prefix of a `jj log --stat` per-file
/// line. Mirrors the JS `STAT_LINE_RE` character class.
const STAT_GRAPH_CHARS: &[char] = &[
    '│', '○', '●', '◆', '~', '*', '╭', '╮', '╯', '╰', '─', '├', '┤', '┬', '┴', '┼',
];

/// If `line` is a commit-header line (graph char + whitespace + change_id),
/// return the change_id. Otherwise `None`.
pub(crate) fn match_commit_header(line: &str) -> Option<&str> {
    let first = line.chars().next()?;
    if !COMMIT_HEADER_CHARS.contains(&first) {
        return None;
    }
    let rest = &line[first.len_utf8()..];
    if !rest.starts_with(|c: char| c.is_whitespace()) {
        return None;
    }
    let after_ws = rest.trim_start();
    let end = after_ws
        .find(|c: char| !c.is_ascii_lowercase())
        .unwrap_or(after_ws.len());
    if end == 0 {
        return None;
    }
    // Reject if the change_id runs into another alphanumeric/underscore char
    // (analogous to the JS regex's trailing `\b`).
    if let Some(next) = after_ws[end..].chars().next()
        && (next.is_alphanumeric() || next == '_')
    {
        return None;
    }
    Some(&after_ws[..end])
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
fn strip_stat_suffix(line: &str) -> Option<&str> {
    let bar = line.rfind(" | ")?;
    let tail = &line[bar + 3..];
    let digits_end = tail
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(tail.len());
    if digits_end == 0 {
        return None;
    }
    let after_digits = &tail[digits_end..];
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
    fn revision_at_line_status_returns_at_when_above_any_commit() {
        let s = status_buffer();
        // Line 1 = "@- : (empty)" — not a commit header, not a stat line.
        // Walking up hits only @  : / @- : lines which don't match
        // match_commit_header → falls through to "@" default.
        assert_eq!(
            revision_at_line(&s, 1, BufferKind::Status).as_deref(),
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
        // Diff context/add/remove lines also walk up
        assert_eq!(file_at_line(&s, 6), None); // " unchanged" — space-prefixed, not a diff op
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
}
