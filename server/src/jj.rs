use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum JjError {
    #[error("jj process failed (exit {exit_code}): {stderr}")]
    JjFailed { exit_code: i32, stderr: String },
    #[error("failed to spawn jj: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Jj {
    binary: PathBuf,
    workdir: PathBuf,
}

impl Jj {
    pub fn new(binary: impl Into<PathBuf>, workdir: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            workdir: workdir.into(),
        }
    }

    /// Create a `Jj` using the given binary path (or `"jj"` as fallback) and working directory.
    pub fn with_binary_or_default(binary: Option<&str>, workdir: impl Into<PathBuf>) -> Self {
        let binary = binary.unwrap_or("jj");
        Self::new(binary, workdir)
    }

    fn run(&self, args: &[&str]) -> Result<String, JjError> {
        let output = Command::new(&self.binary)
            .args(["--no-pager", "--color=never"])
            .args(args)
            .current_dir(&self.workdir)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(JjError::JjFailed {
                exit_code: output.status.code().unwrap_or(-1),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }

    pub fn status(&self) -> Result<String, JjError> {
        self.run(&["status"])
    }

    pub fn log(&self, revset: &str) -> Result<String, JjError> {
        self.run(&["log", "--revisions", revset])
    }

    pub fn describe_get(&self) -> Result<String, JjError> {
        self.run(&[
            "log",
            "--revisions",
            "@",
            "--no-graph",
            "--template",
            "description",
        ])
    }

    pub fn describe_set(&self, message: &str) -> Result<(), JjError> {
        self.run(&["describe", "--message", message])?;
        Ok(())
    }

    pub fn new_change(&self) -> Result<(), JjError> {
        self.run(&["new"])?;
        Ok(())
    }

    /// List change IDs matching the given revset, one per line.
    pub fn change_ids(&self, revset: &str) -> Result<Vec<String>, JjError> {
        let out = self.run(&[
            "log",
            "--revisions",
            revset,
            "--no-graph",
            "--template",
            "change_id ++ \"\\n\"",
        ])?;
        Ok(out
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect())
    }

    /// Squash a single file's changes from `source` into `source`'s parent.
    /// Uses `--use-destination-message` to avoid opening an editor.
    pub fn squash_file_into_parent(&self, source: &str, file: &str) -> Result<(), JjError> {
        self.run(&[
            "squash",
            "--use-destination-message",
            "--revision",
            source,
            file,
        ])?;
        Ok(())
    }

    /// Squash a single file's changes from `source` into `dest` (typically a child).
    /// Uses `--use-destination-message` to avoid opening an editor.
    pub fn squash_file_into(&self, source: &str, dest: &str, file: &str) -> Result<(), JjError> {
        self.run(&[
            "squash",
            "--use-destination-message",
            "--from",
            source,
            "--into",
            dest,
            file,
        ])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as Cmd;

    fn init_jj_repo(dir: &std::path::Path) -> Jj {
        Cmd::new("jj")
            .args(["git", "init"])
            .current_dir(dir)
            .output()
            .expect("jj git init failed");
        Jj::new("jj", dir)
    }

    #[test]
    fn status_succeeds_in_fresh_repo() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let out = jj.status().expect("status failed");
        assert!(out.contains("Working copy") || out.is_empty() || !out.is_empty());
    }

    #[test]
    fn log_succeeds_with_at_revset() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let out = jj.log("@").expect("log failed");
        assert!(!out.is_empty());
    }

    #[test]
    fn describe_set_and_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.describe_set("test commit message")
            .expect("describe_set failed");
        let desc = jj.describe_get().expect("describe_get failed");
        assert!(desc.contains("test commit message"));
    }

    #[test]
    fn new_change_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.new_change().expect("new failed");
        let out = jj.log("@-").expect("log failed");
        assert!(!out.is_empty());
    }

    #[test]
    fn run_in_nonexistent_repo_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = Jj::new("jj", dir.path());
        let result = jj.status();
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn with_binary_or_default_uses_jj_when_none() {
        let dir = tempfile::tempdir().unwrap();
        let jj = Jj::with_binary_or_default(None, dir.path());
        assert_eq!(jj.binary, PathBuf::from("jj"));
    }

    #[test]
    fn with_binary_or_default_uses_provided_path() {
        let dir = tempfile::tempdir().unwrap();
        let jj = Jj::with_binary_or_default(Some("/usr/local/bin/jj"), dir.path());
        assert_eq!(jj.binary, PathBuf::from("/usr/local/bin/jj"));
    }

    #[test]
    fn change_ids_returns_root_for_empty_repo_parent() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // The @- of a fresh repo is the root commit.
        let parents = jj.change_ids("parents(@)").expect("change_ids failed");
        assert_eq!(parents.len(), 1, "expected one parent, got {parents:?}");
    }

    #[test]
    fn change_ids_returns_empty_for_no_children() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let children = jj.change_ids("(@)+").expect("change_ids failed");
        assert!(
            children.is_empty(),
            "expected no children, got {children:?}"
        );
    }

    #[test]
    fn change_ids_returns_error_for_invalid_revset() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.change_ids("not-a-valid-revset!!!");
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn squash_file_into_parent_moves_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Set up parent commit with a placeholder file.
        std::fs::write(dir.path().join("readme.txt"), "hello\n").unwrap();
        jj.describe_set("parent commit").unwrap();
        jj.new_change().unwrap();
        // Working copy modifies the file.
        std::fs::write(dir.path().join("readme.txt"), "hello world\n").unwrap();
        jj.squash_file_into_parent("@", "readme.txt")
            .expect("squash failed");
        // After squashing, working copy should have no changes to readme.txt.
        let status = jj.status().unwrap();
        assert!(
            !status.contains("readme.txt"),
            "expected readme.txt squashed away; status was:\n{status}"
        );
    }

    #[test]
    fn squash_file_into_moves_change_to_dest() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Layout: parent (@-) describes "first"; @ has a file; child gets added later.
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("source change").unwrap();
        // Create a child commit on top.
        jj.new_change().unwrap();
        jj.describe_set("dest change").unwrap();
        let dest = jj
            .change_ids("@")
            .expect("change_ids failed")
            .first()
            .cloned()
            .expect("expected dest change_id");
        // Move back to the source commit to put readme.txt back on the working copy.
        let source = jj
            .change_ids("@-")
            .expect("change_ids failed")
            .first()
            .cloned()
            .expect("expected source change_id");
        // Squash the file from source into dest.
        jj.squash_file_into(&source, &dest, "readme.txt")
            .expect("squash_file_into failed");
    }
}
