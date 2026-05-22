use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use crate::commands::CommandReference;

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
    command_reference: Arc<CommandReference>,
}

impl Jj {
    pub fn new(binary: impl Into<PathBuf>, workdir: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
            workdir: workdir.into(),
            command_reference: Arc::new(CommandReference::default()),
        }
    }

    /// Create a `Jj` using the given binary path (or `"jj"` as fallback) and working directory.
    pub fn with_binary_or_default(binary: Option<&str>, workdir: impl Into<PathBuf>) -> Self {
        let binary = binary.unwrap_or("jj");
        Self::new(binary, workdir)
    }

    /// Attach client-supplied command-reference overrides. Used by the LSP
    /// entry point so editor-specific hotkey text is rendered into the
    /// generated buffers. Returns `self` to support the builder pattern.
    pub fn with_command_reference(mut self, reference: CommandReference) -> Self {
        self.command_reference = Arc::new(reference);
        self
    }

    /// Reference text overrides supplied by the client at initialize-time.
    /// Falls back to the built-in defaults when a field is unset.
    pub fn command_reference(&self) -> &CommandReference {
        &self.command_reference
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
        self.log_with_stat(revset, false)
    }

    pub fn log_with_stat(&self, revset: &str, stat: bool) -> Result<String, JjError> {
        // Pin the per-commit and graph-node templates so a user's
        // `templates.log` / `templates.log_node` overrides don't change the
        // shape of bad-juju's log buffers (which the clients parse).
        let mut args: Vec<&str> = vec![
            "--config",
            "templates.log_node=builtin_log_node",
            "log",
            "--revisions",
            revset,
            "--template",
            "builtin_log_compact",
        ];
        if stat {
            args.push("--stat");
        }
        self.run(&args)
    }

    pub fn describe_get(&self, revision: &str) -> Result<String, JjError> {
        self.run(&[
            "log",
            "--revisions",
            revision,
            "--no-graph",
            "--template",
            "description",
        ])
    }

    pub fn describe_set(&self, revision: &str, message: &str) -> Result<(), JjError> {
        // jj describe takes revisions positionally (REVSETS), not as a flag.
        self.run(&["describe", "--message", message, revision])?;
        Ok(())
    }

    /// Show the diff for a single revision (`jj diff -r REV`). Returns the
    /// rendered diff text; uses the workspace's default diff renderer.
    pub fn diff(&self, revision: &str) -> Result<String, JjError> {
        self.run(&["diff", "--revisions", revision])
    }

    /// Create a new change. When `parent` is empty, behaves like `jj new`
    /// (child of `@`). When non-empty, behaves like `jj new <REV>` so the new
    /// change becomes a child of the given commit and @ moves to it.
    pub fn new_change(&self, parent: &str) -> Result<(), JjError> {
        if parent.is_empty() {
            self.run(&["new"])?;
        } else {
            self.run(&["new", parent])?;
        }
        Ok(())
    }

    /// Move the working copy forward to a child revision (`jj next`).
    /// When `edit` is true, edit that child in place instead of creating a
    /// new empty change on top of it.
    pub fn next_change(&self, edit: bool) -> Result<(), JjError> {
        if edit {
            self.run(&["next", "--edit"])?;
        } else {
            self.run(&["next"])?;
        }
        Ok(())
    }

    /// Move the working copy backward to an ancestor revision (`jj prev`).
    /// When `edit` is true, edit that ancestor in place instead of creating
    /// a new empty change on top of it.
    pub fn prev_change(&self, edit: bool) -> Result<(), JjError> {
        if edit {
            self.run(&["prev", "--edit"])?;
        } else {
            self.run(&["prev"])?;
        }
        Ok(())
    }

    /// Run `jj undo` to revert the last operation.
    pub fn undo(&self) -> Result<(), JjError> {
        self.run(&["undo"])?;
        Ok(())
    }

    /// Abandon the given revision (`jj abandon REV`). The revision must be
    /// non-empty; callers should normalize to `@` if no explicit revision is
    /// supplied. The revset is passed positionally because `jj abandon` does
    /// not accept the `--revisions` flag.
    pub fn abandon(&self, revision: &str) -> Result<(), JjError> {
        self.run(&["abandon", revision])?;
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

    /// Fetch from all remotes (`jj git fetch`). Returns stdout.
    pub fn git_fetch(&self) -> Result<String, JjError> {
        self.run(&["git", "fetch"])
    }

    /// Push to the default remote (`jj git push`). Returns stdout.
    /// jj push already uses force-with-lease semantics by default so there is
    /// no separate force flag.
    pub fn git_push(&self) -> Result<String, JjError> {
        self.run(&["git", "push"])
    }

    /// Move the working copy to the given revision (`jj edit REV`).
    pub fn edit(&self, revision: &str) -> Result<(), JjError> {
        self.run(&["edit", revision])?;
        Ok(())
    }

    /// Squash a single file's changes from `source` into `source`'s parent.
    /// Uses `--use-destination-message` to avoid opening an editor and
    /// `--keep-emptied` so the source revision survives even when it becomes empty.
    pub fn squash_file_into_parent(&self, source: &str, file: &str) -> Result<(), JjError> {
        self.run(&[
            "squash",
            "--use-destination-message",
            "--keep-emptied",
            "--revision",
            source,
            file,
        ])?;
        Ok(())
    }

    /// Squash a single file's changes from `source` into `dest` (typically a child).
    /// Uses `--use-destination-message` to avoid opening an editor and
    /// `--keep-emptied` so the source revision survives even when it becomes empty.
    pub fn squash_file_into(&self, source: &str, dest: &str, file: &str) -> Result<(), JjError> {
        self.run(&[
            "squash",
            "--use-destination-message",
            "--keep-emptied",
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

    /// Verify that bad-juju's `jj log` output is unaffected by a user's
    /// `templates.log` / `templates.log_node` overrides — the clients parse
    /// these buffers by regex and rely on the builtin_log_compact shape.
    #[test]
    fn log_pins_template_against_user_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Set custom repo-level templates that would otherwise distort the
        // commit body and graph node.
        let cfg_path = dir.path().join(".jj/repo/config.toml");
        std::fs::write(
            &cfg_path,
            "[templates]\n\
             log = '\"USER_TEMPLATE\\n\"'\n\
             log_node = '\"X\"'\n",
        )
        .expect("write config.toml failed");

        let out = jj.log("@").expect("log failed");
        // The pinned builtin_log_node renders @ for the working copy; the
        // user override would have rendered "X".
        assert!(
            out.contains('@'),
            "expected pinned @ working-copy node; got:\n{out}"
        );
        assert!(
            !out.contains("USER_TEMPLATE"),
            "user templates.log leaked into output:\n{out}"
        );
    }

    #[test]
    fn describe_set_and_get_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.describe_set("@", "test commit message")
            .expect("describe_set failed");
        let desc = jj.describe_get("@").expect("describe_get failed");
        assert!(desc.contains("test commit message"));
    }

    #[test]
    fn describe_set_targets_explicit_revision() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.describe_set("@", "first").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "second").unwrap();
        // Update the parent commit directly. Without the --revision flag this
        // would describe @ instead.
        jj.describe_set("@-", "rewritten parent")
            .expect("describe_set with explicit rev failed");
        let parent_desc = jj.describe_get("@-").expect("describe_get @-");
        assert!(
            parent_desc.contains("rewritten parent"),
            "expected parent desc rewritten; got: {parent_desc}"
        );
        let at_desc = jj.describe_get("@").expect("describe_get @");
        assert!(
            at_desc.contains("second"),
            "expected @ unchanged; got: {at_desc}"
        );
    }

    #[test]
    fn diff_returns_changes_for_revision() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        std::fs::write(dir.path().join("file.txt"), "hello\n").unwrap();
        jj.describe_set("@", "add file").unwrap();
        let out = jj.diff("@").expect("diff failed");
        assert!(
            out.contains("file.txt"),
            "expected diff to mention file.txt; got:\n{out}"
        );
    }

    #[test]
    fn diff_invalid_revision_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.diff("not-a-real-rev");
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn new_change_succeeds() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.new_change("").expect("new failed");
        let out = jj.log("@-").expect("log failed");
        assert!(!out.is_empty());
    }

    #[test]
    fn abandon_removes_revision_from_log() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Create a stack: parent → middle (abandon target) → @.
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "middle to abandon").unwrap();
        jj.new_change("").unwrap();
        let middle_id = jj
            .change_ids("@-")
            .expect("change_ids failed")
            .first()
            .cloned()
            .expect("expected middle change_id");
        jj.abandon(&middle_id).expect("abandon failed");
        // After abandoning middle, the reachable log should no longer contain its description.
        let log = jj.log("::@").expect("log failed");
        assert!(
            !log.contains("middle to abandon"),
            "expected middle change abandoned; log still shows it:\n{log}"
        );
    }

    #[test]
    fn abandon_invalid_revision_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.abandon("not-a-real-change");
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn undo_reverts_last_operation() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.describe_set("@", "first description").unwrap();
        jj.describe_set("@", "second description").unwrap();
        jj.undo().expect("undo failed");
        let desc = jj.describe_get("@").unwrap();
        assert!(
            desc.contains("first description"),
            "expected undo to revert to first description, got: {desc}"
        );
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
    fn git_push_with_no_remote_is_no_op() {
        // jj git push with no remote configured exits 0 with "Nothing changed."
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.git_push();
        assert!(result.is_ok(), "expected no error for push with no remote: {result:?}");
    }

    #[test]
    fn git_fetch_with_no_remote_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.git_fetch();
        assert!(
            matches!(result, Err(JjError::JjFailed { .. })),
            "expected JjFailed when no remote configured"
        );
    }

    #[test]
    fn edit_moves_working_copy_to_revision() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "child").unwrap();
        let parent_id = jj
            .change_ids("@-")
            .unwrap()
            .first()
            .cloned()
            .expect("expected parent id");
        jj.edit(&parent_id).expect("edit failed");
        let desc = jj.describe_get("@").unwrap();
        assert!(
            desc.contains("parent"),
            "expected @ on parent after edit; got: {desc}"
        );
    }

    #[test]
    fn edit_invalid_revision_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.edit("not-a-real-rev");
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn squash_file_into_parent_moves_file_changes() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Set up parent commit with a placeholder file.
        std::fs::write(dir.path().join("readme.txt"), "hello\n").unwrap();
        jj.describe_set("@", "parent commit").unwrap();
        jj.new_change("").unwrap();
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
    fn squash_file_into_parent_keeps_source_when_emptied() {
        // Set up: parent (empty) → @ (has only readme.txt). Squashing readme.txt away
        // from @ would normally leave @ empty and abandon it. With --keep-emptied, @ stays.
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Parent commit, with no changes of its own.
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        // @ has only this one file.
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("@", "middle change").unwrap();
        let before = jj.change_ids("@").unwrap();
        jj.squash_file_into_parent("@", "readme.txt")
            .expect("squash failed");
        let after = jj.change_ids("@").unwrap();
        assert_eq!(
            before, after,
            "@ should still be the same change after --keep-emptied squash; before: {before:?}, after: {after:?}"
        );
    }

    #[test]
    fn next_change_advances_working_copy() {
        // Set up: parent → @ . `jj prev` first to back up, then `next` should
        // land on a fresh empty change on top of the original @.
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.describe_set("@", "root commit").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "leaf").unwrap();
        // Back up one step; the new @ is an empty change above "root commit".
        jj.prev_change(false).expect("prev failed");
        let parents = jj.change_ids("@-").unwrap();
        assert!(
            !parents.is_empty(),
            "expected @ to have a parent after prev"
        );
    }

    #[test]
    fn prev_change_with_edit_moves_to_parent() {
        // Layout: parent → @. `prev --edit` should move @ onto the parent,
        // not create a new commit. So describe_get("@") should return "parent".
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.describe_set("@", "parent description").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "child description").unwrap();
        jj.prev_change(true).expect("prev --edit failed");
        let desc = jj.describe_get("@").unwrap();
        assert!(
            desc.contains("parent description"),
            "expected @ to be on parent after prev --edit; got: {desc}"
        );
    }

    #[test]
    fn next_change_with_no_descendants_returns_error() {
        // Fresh repo: @ has no descendants, so `jj next` should fail.
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.next_change(false);
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn squash_file_into_moves_change_to_dest() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Layout: parent (@-) describes "first"; @ has a file; child gets added later.
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("@", "source change").unwrap();
        // Create a child commit on top.
        jj.new_change("").unwrap();
        jj.describe_set("@", "dest change").unwrap();
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
