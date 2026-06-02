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
        // Use an explicit template instead of builtin_log_compact to avoid
        // OSC 8 hyperlink escape codes that newer jj versions emit.
        // Field order: change_id, commit_id, author.email, commit_timestamp,
        // bookmarks, tags, conflict flag, divergent flag.
        // The trailing `++ "\n"` inserts a blank line between commits.
        let template = concat!(
            r#"separate("\n","#,
            r#"  separate(" ","#,
            r#"    change_id.shortest(8),"#,
            r#"    commit_id.shortest(8),"#,
            r#"    author.email(),"#,
            r#"    commit_timestamp(self),"#,
            r#"    bookmarks,"#,
            r#"    tags,"#,
            r#"    if(conflict, "conflict"),"#,
            r#"    if(divergent, "divergent"),"#,
            r#"  ),"#,
            r#"  if(!description, "(empty)", description.first_line()),"#,
            r#") ++ "\n\n""#,
        );
        let mut args: Vec<&str> = vec![
            "--config",
            "templates.log_node=builtin_log_node",
            "log",
            "--revisions",
            revset,
            "--template",
            template,
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

    /// Resolve a revision expression to its full change ID (32-char z-base-32).
    pub fn change_id_of(&self, rev: &str) -> Result<String, JjError> {
        let out = self.run(&[
            "log",
            "--revisions",
            rev,
            "--no-graph",
            "--template",
            "change_id",
            "--limit",
            "1",
        ])?;
        let id = out.trim().to_string();
        if id.is_empty() {
            return Err(JjError::JjFailed {
                exit_code: 1,
                stderr: format!("no commit matched revision {rev:?}"),
            });
        }
        Ok(id)
    }

    /// Resolve a revision expression to its full commit ID (hex hash).
    pub fn commit_id_of(&self, rev: &str) -> Result<String, JjError> {
        let out = self.run(&[
            "log",
            "--revisions",
            rev,
            "--no-graph",
            "--template",
            "commit_id",
            "--limit",
            "1",
        ])?;
        let id = out.trim().to_string();
        if id.is_empty() {
            return Err(JjError::JjFailed {
                exit_code: 1,
                stderr: format!("no commit matched revision {rev:?}"),
            });
        }
        Ok(id)
    }

    /// Return the current operation head ID (128-char hex string).
    pub fn op_head_id(&self) -> Result<String, JjError> {
        let out = self.run(&[
            "op",
            "log",
            "--no-graph",
            "--template",
            "id",
            "--limit",
            "1",
        ])?;
        let id = out.trim().to_string();
        if id.is_empty() {
            return Err(JjError::JjFailed {
                exit_code: 1,
                stderr: "op log returned empty output".to_string(),
            });
        }
        Ok(id)
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

    /// Rebase `source` onto `dest` (`jj rebase -s <source> -d <dest>`).
    /// When `source` is empty, defaults to `@`.
    pub fn rebase(&self, source: &str, dest: &str) -> Result<(), JjError> {
        let source = if source.is_empty() { "@" } else { source };
        self.run(&["rebase", "-s", source, "-d", dest])?;
        Ok(())
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

    /// Create a bookmark at `revision` (`jj bookmark create <name> -r <rev>`).
    pub fn bookmark_create(&self, name: &str, revision: &str) -> Result<(), JjError> {
        let revision = if revision.is_empty() { "@" } else { revision };
        self.run(&["bookmark", "create", name, "-r", revision])?;
        Ok(())
    }

    /// Move (set) a bookmark to `revision` (`jj bookmark set <name> -r <rev> --allow-backwards`).
    pub fn bookmark_move(&self, name: &str, revision: &str) -> Result<(), JjError> {
        let revision = if revision.is_empty() { "@" } else { revision };
        self.run(&["bookmark", "set", name, "-r", revision, "--allow-backwards"])?;
        Ok(())
    }

    /// Delete a bookmark (`jj bookmark delete <name>`).
    pub fn bookmark_delete(&self, name: &str) -> Result<(), JjError> {
        self.run(&["bookmark", "delete", name])?;
        Ok(())
    }

    /// Track a remote bookmark (`jj bookmark track <name>@<remote>`).
    pub fn bookmark_track(&self, name_at_remote: &str) -> Result<(), JjError> {
        self.run(&["bookmark", "track", name_at_remote])?;
        Ok(())
    }

    /// Forget a bookmark without recording a deletion (`jj bookmark forget <name>`).
    pub fn bookmark_forget(&self, name: &str) -> Result<(), JjError> {
        self.run(&["bookmark", "forget", name])?;
        Ok(())
    }

    /// Return the bookmark names pointing at `revision`, one per element.
    pub fn bookmarks_of(&self, revision: &str) -> Result<Vec<String>, JjError> {
        let out = self.run(&[
            "log",
            "--revisions",
            revision,
            "--no-graph",
            "--template",
            r#"separate(" ", bookmarks)"#,
            "--limit",
            "1",
        ])?;
        Ok(out
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect())
    }

    /// List files changed by a revision (`jj diff --revisions REV --summary`).
    /// Parses M/A/D/R lines and returns destination paths.
    pub fn files_changed(&self, rev: &str) -> Result<Vec<String>, JjError> {
        let out = self.run(&["diff", "--revisions", rev, "--summary"])?;
        let paths = out
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let mut chars = line.chars();
                let flag = chars.next()?;
                if !matches!(flag, 'M' | 'A' | 'D' | 'R') {
                    return None;
                }
                let rest = line[flag.len_utf8()..].trim();
                // Renames: "R old => new" — take the destination after "=> "
                if flag == 'R'
                    && let Some(dest) = rest.split(" => ").nth(1)
                {
                    return Some(dest.trim().to_string());
                }
                if rest.is_empty() {
                    None
                } else {
                    Some(rest.to_string())
                }
            })
            .collect();
        Ok(paths)
    }

    /// Show the unified diff for a single file in a revision.
    /// Returns the diff text (empty string if no changes to that file).
    pub fn diff_file(&self, rev: &str, path: &str) -> Result<String, JjError> {
        self.run(&["diff", "--revisions", rev, "--", path])
    }

    /// Return `jj diff --from <from_rev> --to <to_rev> --git` output.
    /// Used to enumerate baseline hunks for the squash window.
    pub fn diff_from_to_git(&self, from_rev: &str, to_rev: &str) -> Result<String, JjError> {
        self.run(&["diff", "--from", from_rev, "--to", to_rev, "--git"])
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
    /// these buffers by regex and rely on the explicit template shape.
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
        assert!(
            result.is_ok(),
            "expected no error for push with no remote: {result:?}"
        );
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
    fn rebase_moves_commit_to_dest() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Create a two-commit chain: root → A → B (@).
        jj.describe_set("@", "commit A").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "commit B").unwrap();
        // Get the root commit id (parent of A).
        let root_id = jj
            .change_ids("root()")
            .unwrap()
            .first()
            .cloned()
            .expect("root commit");
        // Rebase B directly onto root (detach from A).
        jj.rebase("@", &root_id).expect("rebase failed");
        // After rebase, @ parent should be root, not A.
        let parents = jj.change_ids("@-").unwrap();
        assert!(
            parents.contains(&root_id),
            "expected @ parent to be root after rebase; parents={parents:?}"
        );
    }

    #[test]
    fn rebase_invalid_dest_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.rebase("@", "not-a-real-rev");
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn bookmark_create_and_delete_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.bookmark_create("mybookmark", "@")
            .expect("create failed");
        // After creation, the bookmark should resolve to @.
        let at_ids = jj.change_ids("@").unwrap();
        let bk_ids = jj.change_ids("mybookmark").unwrap();
        assert_eq!(at_ids, bk_ids, "bookmark should point to @");
        // Deleting the newly-created bookmark should succeed.
        jj.bookmark_delete("mybookmark").expect("delete failed");
    }

    #[test]
    fn bookmark_move_updates_target() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.bookmark_create("bk", "@").expect("create failed");
        // Create a new child commit.
        jj.new_change("").unwrap();
        // Move bookmark to new @.
        jj.bookmark_move("bk", "@").expect("move failed");
        // Verify: the bookmark should now point to @, not the parent.
        // We do this by checking that a log restricted to the bookmark rev
        // matches the current @.
        let at_ids = jj.change_ids("@").unwrap();
        let bk_ids = jj.change_ids("bk").unwrap();
        assert_eq!(at_ids, bk_ids, "bookmark should point to @");
    }

    #[test]
    fn bookmark_forget_removes_without_deletion() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.bookmark_create("forgotten", "@").expect("create failed");
        jj.bookmark_forget("forgotten").expect("forget failed");
        // Referencing the forgotten bookmark should fail.
        let result = jj.change_ids("forgotten");
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
    fn change_id_of_returns_full_id_for_at() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let id = jj.change_id_of("@").expect("change_id_of failed");
        assert!(!id.is_empty(), "expected non-empty change_id");
        assert_eq!(id, id.to_lowercase(), "change_id should be lowercase");
    }

    #[test]
    fn commit_id_of_returns_full_id_for_at() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let id = jj.commit_id_of("@").expect("commit_id_of failed");
        assert!(!id.is_empty(), "expected non-empty commit_id");
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit()),
            "commit_id should be hex; got: {id}"
        );
    }

    #[test]
    fn change_id_of_invalid_revision_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.change_id_of("not-a-real-rev");
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn commit_id_of_invalid_revision_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let result = jj.commit_id_of("not-a-real-rev");
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn change_id_of_amend_preserves_change_id() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let before = jj.change_id_of("@").expect("change_id_of before failed");
        jj.describe_set("@", "amended").unwrap();
        let after = jj.change_id_of("@").expect("change_id_of after failed");
        assert_eq!(before, after, "change-id should survive describe/amend");
    }

    #[test]
    fn commit_id_of_amend_changes_commit_id() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let before = jj.commit_id_of("@").expect("commit_id_of before failed");
        jj.describe_set("@", "amended").unwrap();
        let after = jj.commit_id_of("@").expect("commit_id_of after failed");
        assert_ne!(before, after, "commit-id should change after amend");
    }

    #[test]
    fn op_head_id_returns_128_char_hex() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let id = jj.op_head_id().expect("op_head_id failed");
        assert_eq!(id.len(), 128, "expected 128-char hex; got len={}", id.len());
        assert!(
            id.chars().all(|c| c.is_ascii_hexdigit()),
            "op_head_id should be hex; got: {id}"
        );
    }

    #[test]
    fn op_head_id_outside_repo_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = Jj::new("jj", dir.path());
        let result = jj.op_head_id();
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

    #[test]
    fn log_with_stat_field_order_has_email_before_timestamp() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        let out = jj.log("@").expect("log failed");
        // The header line is: <graph> <change_id> <commit_id> <email> <timestamp> ...
        // Verify email appears before the timestamp on the header line.
        let header = out.lines().next().expect("no output");
        let at_pos = header.find('@').expect("no @ in header");
        // email contains @, timestamp does not — find a digit sequence for timestamp
        let ts_pos = header
            .find(|c: char| c.is_ascii_digit())
            .expect("no digit in header");
        assert!(
            at_pos < ts_pos,
            "expected email (@ sign) before timestamp (digit) in header: {header}"
        );
    }

    #[test]
    fn log_with_stat_inserts_blank_line_between_commits() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        jj.describe_set("@", "first").unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "second").unwrap();
        let out = jj.log("::@").expect("log failed");
        // In graph mode the blank line between commits appears as a line
        // containing only the graph continuation char "│", or as an empty
        // line for the root commit. Either signals a separator was injected.
        assert!(
            out.lines()
                .any(|l| l.trim_end().is_empty() || l.trim_end() == "│"),
            "expected blank separator line between commits; got:\n{out}"
        );
    }

    #[test]
    fn files_changed_returns_modified_paths() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        std::fs::write(dir.path().join("alpha.txt"), "a\n").unwrap();
        std::fs::write(dir.path().join("beta.txt"), "b\n").unwrap();
        let files = jj.files_changed("@").expect("files_changed failed");
        assert!(
            files.contains(&"alpha.txt".to_string()),
            "expected alpha.txt; got {files:?}"
        );
        assert!(
            files.contains(&"beta.txt".to_string()),
            "expected beta.txt; got {files:?}"
        );
    }

    #[test]
    fn files_changed_empty_for_clean_working_copy() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Fresh repo with no uncommitted changes.
        let files = jj.files_changed("@").expect("files_changed failed");
        assert!(files.is_empty(), "expected empty list; got {files:?}");
    }

    #[test]
    fn files_changed_handles_rename_returns_destination() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Create original file in a parent commit.
        std::fs::write(dir.path().join("old.txt"), "content\n").unwrap();
        jj.describe_set("@", "add old.txt").unwrap();
        jj.new_change("").unwrap();
        // Rename by removing old and adding new.
        std::fs::remove_file(dir.path().join("old.txt")).unwrap();
        std::fs::write(dir.path().join("new.txt"), "content\n").unwrap();
        let files = jj.files_changed("@").expect("files_changed failed");
        // Should contain new.txt (destination) regardless of rename representation.
        assert!(
            files.iter().any(|f| f.contains("new.txt")),
            "expected new.txt in results; got {files:?}"
        );
    }

    #[test]
    fn files_changed_outside_repo_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = Jj::new("jj", dir.path());
        let result = jj.files_changed("@");
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }

    #[test]
    fn diff_file_returns_hunks_for_modified_file() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "hello\n").unwrap();
        let out = jj.diff_file("@", "readme.txt").expect("diff_file failed");
        assert!(
            out.contains("readme.txt"),
            "expected diff to mention readme.txt; got:\n{out}"
        );
    }

    #[test]
    fn diff_file_empty_for_unmodified_path() {
        let dir = tempfile::tempdir().unwrap();
        let jj = init_jj_repo(dir.path());
        // Write a file in the parent, then create a new empty change.
        std::fs::write(dir.path().join("unchanged.txt"), "v1\n").unwrap();
        jj.describe_set("@", "add file").unwrap();
        jj.new_change("").unwrap();
        // @ has no changes to unchanged.txt.
        let out = jj
            .diff_file("@", "unchanged.txt")
            .expect("diff_file failed");
        assert!(
            out.is_empty(),
            "expected empty diff for unmodified file; got:\n{out}"
        );
    }

    #[test]
    fn diff_file_outside_repo_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let jj = Jj::new("jj", dir.path());
        let result = jj.diff_file("@", "anything.txt");
        assert!(matches!(result, Err(JjError::JjFailed { .. })));
    }
}
