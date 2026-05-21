use std::path::{Path, PathBuf};

use crate::jj::{Jj, JjError};

const STATUS_REVSET: &str = "ancestors(reachable(@, mutable()), 2)";

const COMMAND_REFERENCE: &str = "\
COMMAND REFERENCE:
n   new
c   commit
l   log
r   rebase";

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
        COMMAND_REFERENCE,
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("status.jj");
    std::fs::write(&path, content)?;
    Ok(file_uri(&path))
}

/// Run `badjuju.log`: write log.jj and return its URI.
pub fn run_log(jj: &Jj, workspace: &Path, revset: &str) -> Result<String, CommandError> {
    let output = jj.log(revset)?;

    let content = format!("REVSET: {}\n\n{}", revset, output.trim_end());

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
    fn run_log_writes_file_with_revset_header() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_log(&jj, dir.path(), "@").expect("run_log failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(content.starts_with("REVSET: @"));
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
}
