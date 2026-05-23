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
    /// URIs that don't end in one of the three known buffer names.
    pub fn from_uri(uri: &str) -> Option<Self> {
        if uri.ends_with("status.jujutsu") {
            Some(Self::Status)
        } else if uri.ends_with("log.jujutsu") {
            Some(Self::Log)
        } else if uri.ends_with("diff.jujutsu") {
            Some(Self::Diff)
        } else {
            None
        }
    }
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
/// - `Status`: STATUS-section file lines (`M src/main.rs` etc.) belong to the
///   working copy. Otherwise walk upward looking for a commit header; if the
///   walk hits the `STATUS:` section header first, fall back to `@`.
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
        if text.starts_with("STATUS:") {
            return "@".to_string();
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
    let rest = first.strip_prefix("REVISION:")?.trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

/// Resolve the file path at a given 0-indexed line of a `status.jujutsu`
/// buffer. Handles both the STATUS-section header form (`M src/main.rs`) and
/// the `jj log --stat` per-file form (`│  src/main.rs | 3 +++`). Renames
/// rendered as `old => new` return only the destination path.
///
/// Returns `None` for blank lines, header lines, the `--stat` summary line
/// (`5 files changed, ...`), or any line that doesn't match either form.
pub fn file_at_line(content: &str, line: usize) -> Option<String> {
    let line_text = content.lines().nth(line)?;
    parse_file_line(line_text)
}

/// Resolve a `JJ: <Label>: <revset>` shortcut at the given 0-indexed line of
/// a `log.jujutsu` buffer. Returns `None` if the line isn't a shortcut line.
pub fn log_shortcut_at_line(content: &str, line: usize) -> Option<LogShortcut> {
    let line_text = content.lines().nth(line)?;
    parse_log_shortcut(line_text)
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
fn match_commit_header(line: &str) -> Option<&str> {
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
fn parse_status_header_line(line: &str) -> Option<String> {
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
        assert_eq!(
            BufferKind::from_uri("file:///x/diff.jujutsu"),
            Some(BufferKind::Diff)
        );
        assert_eq!(BufferKind::from_uri("file:///x/other.txt"), None);
    }

    // --- revision_at_line: Status buffer ---

    fn status_buffer() -> String {
        // Mirrors the shape written by commands::write_status:
        //   STATUS: header, status lines, blank line, STACK: line, log output.
        [
            "STATUS:",
            "",
            "Working copy changes:",
            "M src/main.rs",
            "A new.txt",
            "R old.txt => renamed.txt",
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
    fn revision_at_line_status_returns_at_on_status_file_line() {
        let s = status_buffer();
        // Line 3 = "M src/main.rs"
        assert_eq!(
            revision_at_line(&s, 3, BufferKind::Status).as_deref(),
            Some("@")
        );
    }

    #[test]
    fn revision_at_line_status_returns_at_when_above_any_commit() {
        let s = status_buffer();
        // Line 2 = "Working copy changes:" — between STATUS: and the file lines.
        // Walking up hits STATUS: → "@".
        assert_eq!(
            revision_at_line(&s, 2, BufferKind::Status).as_deref(),
            Some("@")
        );
    }

    #[test]
    fn revision_at_line_status_returns_change_id_on_commit_header() {
        let s = status_buffer();
        // Line 9 = "@  qpvuntsm 1234abcd"
        assert_eq!(
            revision_at_line(&s, 9, BufferKind::Status).as_deref(),
            Some("qpvuntsm")
        );
        // Line 11 = "○  abcdwxyz e6f7e6f7"
        assert_eq!(
            revision_at_line(&s, 11, BufferKind::Status).as_deref(),
            Some("abcdwxyz")
        );
    }

    #[test]
    fn revision_at_line_status_walks_up_to_nearest_commit() {
        let s = status_buffer();
        // Line 12 = "│  another commit" — should walk up to abcdwxyz.
        assert_eq!(
            revision_at_line(&s, 12, BufferKind::Status).as_deref(),
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
    fn revision_at_line_diff_returns_header_revision() {
        let content = "REVISION: abc123\n\nDIFF:\n\n@@ -1 +1 @@\n-foo\n+bar\n";
        assert_eq!(
            revision_at_line(content, 5, BufferKind::Diff).as_deref(),
            Some("abc123")
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
}
