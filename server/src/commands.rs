use std::path::{Path, PathBuf};

use crate::jj::{Jj, JjError};

const STATUS_REVSET: &str = "ancestors(reachable(@, mutable()), 2)";

const STATUS_COMMAND_REFERENCE: &str = "\
COMMAND REFERENCE:
n   new change
l   open log
d   describe
s   squash file at cursor into parent
u   jj undo (revert last operation)
=   toggle --stat on the stack log
g   refresh
q   close";

const LOG_COMMAND_REFERENCE: &str = "\
COMMAND REFERENCE:
Edit REVSET above and save to re-run the query.
Place the cursor on a shortcut line and press Enter to apply it.";

/// Pre-defined revset shortcuts shown in the log.jj header.
/// Each entry is (label, revset). The label is also used to align columns.
const LOG_SHORTCUTS: &[(&str, &str)] = &[
    ("Mutable", "ancestors(reachable(@, mutable()))"),
    ("Stack", "(immutable_heads()..@)::"),
];

/// Render the shortcut list as `JJ:` comment lines for the log.jj header.
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

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Run `badjuju.status`: write status.jj (preserving any current STATS toggle) and return its URI.
pub fn run_status(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    write_status(jj, workspace, None, stat)
}

/// Toggle the STATS marker in status.jj and re-render.
pub fn run_toggle_stat(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    let next = !read_current_stat(workspace);
    write_status(jj, workspace, None, next)
}

/// Read the STATS marker from the current status.jj if it exists, defaulting to `false`.
fn read_current_stat(workspace: &Path) -> bool {
    let Ok(dir) = badjuju_dir(workspace) else {
        return false;
    };
    let path = dir.join("status.jj");
    std::fs::read_to_string(&path)
        .ok()
        .as_deref()
        .and_then(parse_status_stats)
        .unwrap_or(false)
}

/// Extract the STATS marker from a status.jj buffer. Returns `Some(true)` for "on",
/// `Some(false)` for "off", and `None` if the marker is missing or unrecognized.
pub fn parse_status_stats(content: &str) -> Option<bool> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("STATS: ") {
            return match rest.trim() {
                "on" => Some(true),
                "off" => Some(false),
                _ => None,
            };
        }
    }
    None
}

/// Write status.jj, optionally prepending a MESSAGE: block. Returns the URI.
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
    let stats_marker = if stat { "on" } else { "off" };

    let content = format!(
        "{}STATUS:\n\n{}\n\nSTACK: {}\nSTATS: {}\n\n{}\n\n{}",
        prelude,
        status.trim_end(),
        STATUS_REVSET,
        stats_marker,
        stack.trim_end(),
        STATUS_COMMAND_REFERENCE,
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("status.jj");
    std::fs::write(&path, content)?;
    Ok(file_uri(&path))
}

/// Run `badjuju.squash`: move the file's changes from @ into @-, then refresh status.
/// If @ has multiple parents, no action is taken and the status buffer reports the error.
pub fn run_squash(jj: &Jj, workspace: &Path, file: &str) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    if file.is_empty() {
        return write_status(jj, workspace, Some("squash: no file selected"), stat);
    }
    let parents = jj.change_ids("parents(@)")?;
    if parents.len() != 1 {
        return write_status(
            jj,
            workspace,
            Some(&format!(
                "squash {file}: working copy has {} parents (need exactly 1)",
                parents.len()
            )),
            stat,
        );
    }
    match jj.squash_file_into_parent("@", file) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("squash {file} failed: {e}")),
            stat,
        ),
    }
}

/// Run `badjuju.unsquash`: move the file's changes from @ into its child, then refresh status.
/// If @ has zero or multiple children, no action is taken and the status buffer reports the error.
pub fn run_unsquash(jj: &Jj, workspace: &Path, file: &str) -> Result<String, CommandError> {
    let stat = read_current_stat(workspace);
    if file.is_empty() {
        return write_status(jj, workspace, Some("unsquash: no file selected"), stat);
    }
    let children = jj.change_ids("(@)+")?;
    if children.len() != 1 {
        return write_status(
            jj,
            workspace,
            Some(&format!(
                "unsquash {file}: working copy has {} children (need exactly 1)",
                children.len()
            )),
            stat,
        );
    }
    match jj.squash_file_into("@", &children[0], file) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("unsquash {file} failed: {e}")),
            stat,
        ),
    }
}

/// Run `badjuju.log`: write log.jj and return its URI.
pub fn run_log(jj: &Jj, workspace: &Path, revset: &str) -> Result<String, CommandError> {
    let output = jj.log(revset)?;

    let content = format!(
        "REVSET: {}\n{}\n\nOUTPUT:\n\n{}\n\n{}",
        revset,
        render_log_shortcuts(),
        output.trim_end(),
        LOG_COMMAND_REFERENCE,
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("log.jj");
    std::fs::write(&path, content)?;
    Ok(file_uri(&path))
}

/// Run `badjuju.describe`: write describe.jj with current description and return its URI.
pub fn run_describe(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    let current_desc = jj.describe_get()?;
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
         JJ: Lines starting with 'JJ:' will be removed.\n",
        desc,
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("describe.jj");
    std::fs::write(&path, content)?;
    Ok(file_uri(&path))
}

/// Run `badjuju.refresh`: regenerate the file identified by `uri`.
/// For status.jj → regenerate status. For log.jj → re-run log with current REVSET header.
pub fn run_refresh(jj: &Jj, workspace: &Path, uri: &str) -> Result<String, CommandError> {
    let path = uri.strip_prefix("file://").unwrap_or(uri);
    let filename = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    match filename {
        "log.jj" => {
            let content = std::fs::read_to_string(path)?;
            let revset = parse_log_revset(&content).unwrap_or_else(|| "@".to_string());
            run_log(jj, workspace, &revset)
        }
        _ => run_status(jj, workspace),
    }
}

/// Run `badjuju.new`: create a new change and regenerate status.jj. Returns the status URI.
pub fn run_new(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    jj.new_change()?;
    run_status(jj, workspace)
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

/// Strip JJ: comment lines and the separator from describe.jj content.
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

/// Extract the revset from the `REVSET: <revset>` header line of log.jj.
pub fn parse_log_revset(content: &str) -> Option<String> {
    content
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("REVSET: "))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// On describe.jj save: apply stripped description via jj describe, then regenerate status.jj.
pub fn on_describe_save(jj: &Jj, workspace: &Path, content: &str) -> Result<(), CommandError> {
    if let Some(desc) = parse_describe_content(content) {
        jj.describe_set(&desc)?;
        run_status(jj, workspace)?;
    }
    Ok(())
}

/// On log.jj save: re-parse the REVSET: header and regenerate the file.
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
        for key in [
            "n   new",
            "l   open log",
            "d   describe",
            "s   squash",
            "u   jj undo",
            "g   refresh",
            "q   close",
        ] {
            assert!(
                content.contains(key),
                "missing `{key}` in status command reference:\n{content}"
            );
        }
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
            content.contains("ancestors(reachable(@, mutable()))"),
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
        // Simulate a saved log.jj that still contains the shortcut comment lines.
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
        let uri = run_describe(&jj, dir.path()).expect("run_describe failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("JJ:"));
        assert!(content.contains(">8"));
    }

    #[test]
    fn run_describe_roundtrips_description() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("my feature work").unwrap();
        let uri = run_describe(&jj, dir.path()).expect("run_describe failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.contains("my feature work"));
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
    fn on_describe_save_applies_description() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let content = "new description\n\nJJ: separator\n";
        on_describe_save(&jj, dir.path(), content).expect("on_describe_save failed");
        let desc = jj.describe_get().unwrap();
        assert!(desc.contains("new description"));
    }

    #[test]
    fn on_describe_save_skips_empty_content() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("original description").unwrap();
        let content = "\n\nJJ: separator\n";
        on_describe_save(&jj, dir.path(), content).expect("on_describe_save failed");
        let desc = jj.describe_get().unwrap();
        assert!(desc.contains("original description"));
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
        let uri = run_new(&jj, dir.path()).expect("run_new failed");
        assert!(uri.starts_with("file://"));
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.contains("STATUS:"));
    }

    #[test]
    fn run_new_creates_new_change_in_log() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let log_before = jj.log("@").unwrap();
        run_new(&jj, dir.path()).expect("run_new failed");
        let log_after = jj.log("@").unwrap();
        assert_ne!(log_before, log_after);
    }

    #[test]
    fn run_squash_with_empty_file_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_squash(&jj, dir.path(), "").expect("run_squash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("MESSAGE: squash: no file selected"));
    }

    #[test]
    fn run_squash_moves_file_into_parent_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Parent commit has the file with one content.
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("parent").unwrap();
        jj.new_change().unwrap();
        // Working copy modifies the file.
        std::fs::write(dir.path().join("readme.txt"), "v2\n").unwrap();
        let uri = run_squash(&jj, dir.path(), "readme.txt").expect("run_squash failed");
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
        let uri = run_squash(&jj, dir.path(), "does-not-exist.txt").expect("run_squash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: squash does-not-exist.txt failed:"),
            "expected error message, got:\n{content}"
        );
    }

    #[test]
    fn run_unsquash_with_no_children_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        // @ has no children — unsquash should fail with descriptive message.
        let uri = run_unsquash(&jj, dir.path(), "readme.txt").expect("run_unsquash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("MESSAGE: unsquash readme.txt: working copy has 0 children"),
            "got:\n{content}"
        );
    }

    #[test]
    fn run_unsquash_with_empty_file_reports_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_unsquash(&jj, dir.path(), "").expect("run_unsquash failed");
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
        jj.describe_set("source").unwrap();
        // Create a child commit so @ has exactly one child after we edit back.
        jj.new_change().unwrap();
        jj.describe_set("child").unwrap();
        // Move back to source so @ has the child we just created.
        Command::new("jj")
            .args(["edit", "@-"])
            .current_dir(dir.path())
            .output()
            .expect("jj edit failed");
        let uri = run_unsquash(&jj, dir.path(), "readme.txt").expect("run_unsquash failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("STATUS:"));
    }

    #[test]
    fn run_undo_reverts_last_operation_and_refreshes_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("first").unwrap();
        jj.describe_set("second").unwrap();
        let uri = run_undo(&jj, dir.path()).expect("run_undo failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.starts_with("STATUS:"));
        let desc = jj.describe_get().unwrap();
        assert!(
            desc.contains("first"),
            "expected undo to roll back to first; got: {desc}"
        );
    }

    #[test]
    fn run_undo_preserves_stat_state() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        run_toggle_stat(&jj, dir.path()).unwrap();
        jj.describe_set("a description").unwrap();
        let uri = run_undo(&jj, dir.path()).expect("run_undo failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.contains("STATS: on"),
            "undo should preserve STATS: on:\n{content}"
        );
    }

    #[test]
    fn run_status_defaults_to_stat_off() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_status(&jj, dir.path()).unwrap();
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.contains("STATS: off"),
            "expected STATS: off:\n{content}"
        );
    }

    #[test]
    fn toggle_stat_flips_marker() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        run_status(&jj, dir.path()).unwrap();
        let uri = run_toggle_stat(&jj, dir.path()).unwrap();
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.contains("STATS: on"),
            "expected STATS: on after toggle:\n{content}"
        );
        let uri = run_toggle_stat(&jj, dir.path()).unwrap();
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.contains("STATS: off"),
            "expected STATS: off after second toggle:\n{content}"
        );
    }

    #[test]
    fn run_status_preserves_stat_across_calls() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        run_toggle_stat(&jj, dir.path()).unwrap(); // stat on
        let uri = run_status(&jj, dir.path()).unwrap();
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.contains("STATS: on"),
            "stat should be preserved:\n{content}"
        );
    }

    #[test]
    fn parse_status_stats_recognizes_on_and_off() {
        assert_eq!(parse_status_stats("STATS: on\n"), Some(true));
        assert_eq!(parse_status_stats("STATS: off\n"), Some(false));
        assert_eq!(parse_status_stats("STATUS:\nSTATS: on\n"), Some(true));
        assert_eq!(parse_status_stats("no marker here"), None);
        assert_eq!(parse_status_stats("STATS: weird\n"), None);
    }

    #[test]
    fn squash_preserves_stat_state() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("parent").unwrap();
        jj.new_change().unwrap();
        std::fs::write(dir.path().join("readme.txt"), "v2\n").unwrap();
        run_toggle_stat(&jj, dir.path()).unwrap(); // stat on
        let uri = run_squash(&jj, dir.path(), "readme.txt").unwrap();
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.contains("STATS: on"),
            "squash should preserve STATS: on:\n{content}"
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
}
