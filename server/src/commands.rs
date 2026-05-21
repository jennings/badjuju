use std::path::{Path, PathBuf};

use crate::jj::{Jj, JjError};

const STATUS_REVSET: &str = "ancestors(reachable(@, mutable()), 2)";

const STATUS_COMMAND_REFERENCE: &str = "\
COMMAND REFERENCE:
n   new change
l   open log
d   describe
g   refresh
q   close";

const LOG_COMMAND_REFERENCE: &str = "\
COMMAND REFERENCE:
Edit REVSET above and save to re-run the query.";

/// Returns the `<workspace>/.jj/badjuju/` directory, creating it if needed.
fn badjuju_dir(workspace: &Path) -> std::io::Result<PathBuf> {
    let dir = workspace.join(".jj").join("badjuju");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn file_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

/// Run `badjuju.status`: write status.jj and return its URI.
pub fn run_status(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    let status = jj.status()?;
    let stack = jj.log(STATUS_REVSET)?;

    let content = format!(
        "STATUS:\n\n{}\n\nSTACK: {}\n\n{}\n\n{}",
        status.trim_end(),
        STATUS_REVSET,
        stack.trim_end(),
        STATUS_COMMAND_REFERENCE,
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("status.jj");
    std::fs::write(&path, content)?;
    Ok(file_uri(&path))
}

/// Run `badjuju.log`: write log.jj and return its URI.
pub fn run_log(jj: &Jj, workspace: &Path, revset: &str) -> Result<String, CommandError> {
    let output = jj.log(revset)?;

    let content = format!(
        "REVSET: {}\n\nOUTPUT:\n\n{}\n\n{}",
        revset,
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
