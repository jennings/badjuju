use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tower_lsp::lsp_types::{FoldingRange, Url};

use crate::cursor::{self, BufferKind};
use crate::jj::{Jj, JjError};
use crate::keymap::{self, KeymapProfile};

const STATUS_REVSET: &str = "ancestors(reachable(@, mutable()), 2)";

/// Default revset for the log window when the client passes no explicit revset.
const DEFAULT_LOG_REVSET: &str = "ancestors(mutable(), 2)";

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
    squash: String,
    hunk_edit: String,
}

impl Default for CommandReference {
    fn default() -> Self {
        Self::from_profile(&KeymapProfile::Magit)
    }
}

impl CommandReference {
    /// Render all buffer reference texts from the given profile.
    pub fn from_profile(profile: &KeymapProfile) -> Self {
        Self {
            status: keymap::render_command_reference(profile, "status"),
            log: keymap::render_command_reference(profile, "log"),
            diff: keymap::render_command_reference(profile, "diff"),
            squash: keymap::render_command_reference(profile, "squash"),
            hunk_edit: keymap::render_command_reference(profile, "hunk-edit"),
        }
    }

    /// Build from a profile with optional per-buffer client overrides.
    ///
    /// `None` for a field means "use the profile default"; a `Some` value
    /// replaces the rendered text entirely (the escape-hatch path used by
    /// clients whose keybindings differ from any built-in profile).
    pub fn new(
        status: Option<String>,
        log: Option<String>,
        diff: Option<String>,
        squash: Option<String>,
        hunk_edit: Option<String>,
    ) -> Self {
        Self::with_profile(&KeymapProfile::Magit, status, log, diff, squash, hunk_edit)
    }

    pub fn with_profile(
        profile: &KeymapProfile,
        status: Option<String>,
        log: Option<String>,
        diff: Option<String>,
        squash: Option<String>,
        hunk_edit: Option<String>,
    ) -> Self {
        let base = Self::from_profile(profile);
        Self {
            status: status.unwrap_or(base.status),
            log: log.unwrap_or(base.log),
            diff: diff.unwrap_or(base.diff),
            squash: squash.unwrap_or(base.squash),
            hunk_edit: hunk_edit.unwrap_or(base.hunk_edit),
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

    pub fn squash(&self) -> &str {
        &self.squash
    }

    pub fn hunk_edit(&self) -> &str {
        &self.hunk_edit
    }
}

/// Pre-defined revset shortcuts shown in the log.jujutsu header.
/// Each entry is (label, revset). The label is also used to align columns.
const LOG_SHORTCUTS: &[(&str, &str)] = &[
    ("Mutable", "ancestors(mutable(), 2)"),
    ("Slice", "ancestors(reachable(@, mutable()), 2)"),
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
pub fn path_from_uri(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

/// Run `badjuju.status`: write status.jujutsu and return its URI.
pub fn run_status(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    run_status_with_content(jj, workspace).map(|(uri, _)| uri)
}

/// Same as [`run_status`], but additionally returns the content written to
/// disk so callers can ship it to clients without re-reading the file.
pub fn run_status_with_content(
    jj: &Jj,
    workspace: &Path,
) -> Result<(String, String), CommandError> {
    write_status_with_content(jj, workspace, None)
}

/// Locate the (zero-based) line of a change-id row in rendered status or
/// log content. Matches the second whitespace-separated token of each line
/// (jj's standard log template puts the graph glyph at index 0 and the
/// short change-id at index 1) and accepts any short-id that is a prefix
/// of the full change-id we're looking for.
///
/// Returns `None` when the change-id doesn't appear in the buffer (e.g.
/// the source is outside the current revset).
pub fn find_change_id_line(content: &str, change_id: &str) -> Option<u32> {
    for (idx, line) in content.lines().enumerate() {
        let token = line.split_whitespace().nth(1).unwrap_or("");
        if !token.is_empty() && change_id.starts_with(token) {
            return Some(idx as u32);
        }
    }
    None
}

/// Build a single `@  :` or `@- :` header line for the status buffer.
///
/// Bookmarks are formatted with angle brackets: `<main> <origin>`.
/// Description is truncated to the first line; empty descriptions show `(empty)`.
fn header_line_for(jj: &Jj, rev: &str, marker: &str) -> Result<String, CommandError> {
    let desc_raw = jj.describe_get(rev)?;
    let desc = desc_raw.trim();
    let desc_display = if desc.is_empty() {
        "(empty)".to_string()
    } else {
        desc.lines().next().unwrap_or("(empty)").to_string()
    };

    let bookmarks = jj.bookmarks_of(rev)?;
    if bookmarks.is_empty() {
        Ok(format!("{marker}: {desc_display}"))
    } else {
        let bk_str: String = bookmarks
            .iter()
            .map(|b| format!("<{b}>"))
            .collect::<Vec<_>>()
            .join(" ");
        Ok(format!("{marker}: {bk_str} {desc_display}"))
    }
}

/// Strip diff header lines and return only hunk content for a file in a revision.
///
/// Drops lines starting with `diff --git `, `index `, `--- `, `+++ ` and
/// returns `@@` hunk headers plus their `+`/`-`/` ` content lines.
fn hunks_for(jj: &Jj, rev: &str, path: &str) -> String {
    let Ok(diff) = jj.diff_file(rev, path) else {
        return String::new();
    };
    let lines: Vec<&str> = diff
        .lines()
        .filter(|l| {
            !l.starts_with("diff --git ")
                && !l.starts_with("index ")
                && !l.starts_with("--- ")
                && !l.starts_with("+++ ")
        })
        .collect();
    lines.join("\n")
}

/// Build the WORKING COPY CHANGES / PARENT CHANGES sections for the status buffer.
///
/// Returns an empty string when no revision has any changed files. Each
/// section is emitted only when the revision has at least one changed file.
fn changes_sections(jj: &Jj) -> Result<String, CommandError> {
    let at_id = jj.change_id_of("@")?;
    let at_short = &at_id[..at_id.len().min(8)];
    let at_files = jj.files_changed("@")?;

    let parent_ids = jj.change_ids("parents(@)")?;
    let parents_with_files: Vec<(String, Vec<String>)> = parent_ids
        .into_iter()
        .map(|id| {
            let files = jj.files_changed(&id)?;
            Ok((id, files))
        })
        .collect::<Result<Vec<_>, CommandError>>()?;

    let has_any = !at_files.is_empty() || parents_with_files.iter().any(|(_, f)| !f.is_empty());
    if !has_any {
        return Ok(String::new());
    }

    let mut sections = Vec::new();

    if !at_files.is_empty() {
        let mut s = format!("WORKING COPY CHANGES ({}):", at_short);
        for f in &at_files {
            s.push('\n');
            s.push_str(f);
            let hunks = hunks_for(jj, "@", f);
            if !hunks.is_empty() {
                s.push('\n');
                s.push_str(&hunks);
            }
        }
        sections.push(s);
    }

    for (id, files) in &parents_with_files {
        if files.is_empty() {
            continue;
        }
        let short = &id[..id.len().min(8)];
        let mut s = format!("PARENT CHANGES ({}):", short);
        for f in files {
            s.push('\n');
            s.push_str(f);
            let hunks = hunks_for(jj, id, f);
            if !hunks.is_empty() {
                s.push('\n');
                s.push_str(&hunks);
            }
        }
        sections.push(s);
    }

    Ok(sections.join("\n\n"))
}

/// Write status.jujutsu, optionally prepending a MESSAGE: block. Returns the URI.
pub fn write_status(
    jj: &Jj,
    workspace: &Path,
    message: Option<&str>,
) -> Result<String, CommandError> {
    write_status_with_content(jj, workspace, message).map(|(uri, _)| uri)
}

/// Same as [`write_status`], but additionally returns the content written to
/// disk so callers can ship it to clients without re-reading the file.
pub fn write_status_with_content(
    jj: &Jj,
    workspace: &Path,
    message: Option<&str>,
) -> Result<(String, String), CommandError> {
    let at_header = header_line_for(jj, "@", "@  ")?;

    let parents = jj.change_ids("parents(@)")?;
    let parent_headers = parents
        .iter()
        .map(|id| header_line_for(jj, id, "@- "))
        .collect::<Result<Vec<_>, _>>()?;

    let header_block = std::iter::once(at_header)
        .chain(parent_headers)
        .collect::<Vec<_>>()
        .join("\n");

    let changes = changes_sections(jj)?;
    let stack = jj.log_with_stat(STATUS_REVSET, true)?;

    let prelude = match message {
        Some(m) => format!("MESSAGE: {}\n\n", m.trim()),
        None => String::new(),
    };

    let content = if changes.is_empty() {
        format!(
            "{}{}\n\nSTACK: {}\n\n{}\n\n{}",
            prelude,
            header_block,
            STATUS_REVSET,
            stack.trim_end(),
            jj.command_reference().status(),
        )
    } else {
        format!(
            "{}{}\n\n{}\n\nSTACK: {}\n\n{}\n\n{}",
            prelude,
            header_block,
            changes,
            STATUS_REVSET,
            stack.trim_end(),
            jj.command_reference().status(),
        )
    };

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("status.jujutsu");
    std::fs::write(&path, &content)?;
    Ok((file_uri(&path), content))
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
    let rev = revision_or_at(revision);
    if file.is_empty() {
        return write_status(jj, workspace, Some("squash: no file selected"));
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
        );
    }
    match jj.squash_file_into_parent(rev, file) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("squash {file} from {rev} failed: {e}")),
        ),
    }
}

/// Run `badjuju.squash` from the working-copy (`@`), respecting multi-parent merges.
///
/// - Single parent: squash directly into it (existing behaviour).
/// - Multiple parents, file touched by exactly one: squash into that parent automatically.
/// - Multiple parents, ambiguous or zero: return `RequiresParentSelection`.
pub fn run_squash_working_copy(
    jj: &Jj,
    workspace: &Path,
    file: &str,
) -> Result<String, CommandError> {
    if file.is_empty() {
        return write_status(jj, workspace, Some("squash: no file selected"));
    }
    let parents = jj.change_ids("parents(@)")?;
    match parents.len() {
        0 => write_status(jj, workspace, Some("squash: @ has no parents")),
        1 => match jj.squash_file_into_parent("@", file) {
            Ok(()) => run_status(jj, workspace),
            Err(e) => write_status(jj, workspace, Some(&format!("squash {file} failed: {e}"))),
        },
        _ => {
            // Find which parents have this file in their diff.
            let mut parents_with_file: Vec<String> = parents
                .iter()
                .filter(|p| {
                    jj.files_changed(p)
                        .unwrap_or_default()
                        .iter()
                        .any(|f| f == file)
                })
                .cloned()
                .collect();
            if parents_with_file.len() == 1 {
                let parent_id = parents_with_file.remove(0);
                match jj.squash_file_into("@", &parent_id, file) {
                    Ok(()) => run_status(jj, workspace),
                    Err(e) => write_status(
                        jj,
                        workspace,
                        Some(&format!("squash {file} into {parent_id} failed: {e}")),
                    ),
                }
            } else {
                // Ambiguous (0 or 2+ parents have the file): need user to pick.
                let candidates = parents
                    .iter()
                    .map(|p| {
                        let short = &p[..p.len().min(8)];
                        let desc = jj.describe_get(p).unwrap_or_default();
                        let label = if desc.trim().is_empty() {
                            format!("{short}: (no description)")
                        } else {
                            format!("{short}: {}", desc.trim())
                        };
                        (p.clone(), label)
                    })
                    .collect();
                Err(CommandError::RequiresParentSelection {
                    file: file.to_string(),
                    candidates,
                })
            }
        }
    }
}

/// Run `badjuju.squash.into`: squash `file` from `@` directly into the
/// named `parent_id`. Used after the client resolves an ambiguous multi-parent
/// case via a picker.
pub fn run_squash_into(
    jj: &Jj,
    workspace: &Path,
    file: &str,
    parent_id: &str,
) -> Result<String, CommandError> {
    if file.is_empty() {
        return write_status(jj, workspace, Some("squash: no file selected"));
    }
    if parent_id.is_empty() {
        return write_status(jj, workspace, Some("squash: no parent selected"));
    }
    match jj.squash_file_into("@", parent_id, file) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("squash {file} into {parent_id} failed: {e}")),
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
    let rev = revision_or_at(revision);
    if file.is_empty() {
        return write_status(jj, workspace, Some("unsquash: no file selected"));
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
        );
    }
    match jj.squash_file_into(rev, &children[0], file) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(
            jj,
            workspace,
            Some(&format!("unsquash {file} from {rev} failed: {e}")),
        ),
    }
}

/// Run `badjuju.log`: write log.jujutsu and return its URI.
pub fn run_log(jj: &Jj, workspace: &Path, revset: &str) -> Result<String, CommandError> {
    run_log_with_content(jj, workspace, revset).map(|(uri, _)| uri)
}

/// Same as [`run_log`], but additionally returns the content written to disk.
pub fn run_log_with_content(
    jj: &Jj,
    workspace: &Path,
    revset: &str,
) -> Result<(String, String), CommandError> {
    let revset = if revset.is_empty() {
        DEFAULT_LOG_REVSET
    } else {
        revset
    };
    let output = jj.log_with_stat(revset, true)?;

    let content = format!(
        "REVSET: {}\n{}\n\nOUTPUT:\n\n{}\n\n{}",
        revset,
        render_log_shortcuts(),
        output.trim_end(),
        jj.command_reference().log(),
    );

    let dir = badjuju_dir(workspace)?;
    let path = dir.join("log.jujutsu");
    std::fs::write(&path, &content)?;
    Ok((file_uri(&path), content))
}

/// A single unified-diff hunk inside a squash window buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub file: String,
    pub header: String,
    pub content: String,
}

/// State of an open squash window.
#[derive(Debug, Clone)]
pub struct SquashWindow {
    /// Full change-id of the source revision.
    pub from: String,
    /// Full change-id of the destination revision.
    pub into: String,
    /// `file://` URI of the on-disk squash buffer.
    pub uri: String,
    /// Baseline hunks enumerated from `jj diff --from <from>- --to <from> --git`.
    pub baseline_hunks: Vec<Hunk>,
}

/// Parse git-format unified diff (`--git`) into a flat list of hunks.
/// Each hunk knows its file path, `@@` header line, and content (+/-/space) lines.
pub fn parse_git_diff_hunks(diff: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let mut current_file: Option<String> = None;
    let mut current_header: Option<String> = None;
    let mut current_content: Vec<&str> = Vec::new();

    let flush = |hunks: &mut Vec<Hunk>,
                 current_file: &Option<String>,
                 current_header: &mut Option<String>,
                 current_content: &mut Vec<&str>| {
        if let (Some(file), Some(header)) = (current_file.as_deref(), current_header.take()) {
            hunks.push(Hunk {
                file: file.to_string(),
                header,
                content: current_content.join("\n"),
            });
            current_content.clear();
        }
    };

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            flush(
                &mut hunks,
                &current_file,
                &mut current_header,
                &mut current_content,
            );
            // "diff --git a/path b/path" — extract destination path (after " b/")
            let file = line
                .rsplit_once(" b/")
                .map(|(_, p)| p.to_string())
                .unwrap_or_default();
            current_file = Some(file);
            current_header = None;
            current_content.clear();
        } else if line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("new file mode")
            || line.starts_with("deleted file mode")
            || line.starts_with("old mode")
            || line.starts_with("new mode")
            || line.starts_with("similarity index")
            || line.starts_with("rename from")
            || line.starts_with("rename to")
            || line.starts_with("copy from")
            || line.starts_with("copy to")
        {
            // Git diff metadata lines — skip
        } else if line.starts_with("@@") {
            flush(
                &mut hunks,
                &current_file,
                &mut current_header,
                &mut current_content,
            );
            current_header = Some(line.to_string());
        } else if current_header.is_some() {
            current_content.push(line);
        }
    }
    flush(
        &mut hunks,
        &current_file,
        &mut current_header,
        &mut current_content,
    );
    hunks
}

/// Write a `squash/<from12>-<into12>.jujutsu` buffer showing baseline hunks
/// grouped into SELECTED CHANGES (empty at open time) and REMAINING CHANGES
/// (all hunks). Returns the file URI and the populated `SquashWindow` state.
pub fn run_squash_window(
    jj: &Jj,
    workspace: &Path,
    from: &str,
    into: &str,
) -> Result<(String, SquashWindow), CommandError> {
    let from_commit_id = jj.commit_id_of(from)?;
    let into_commit_id = jj.commit_id_of(into)?;
    let from_desc = jj.describe_get(from)?;
    let into_desc = jj.describe_get(into)?;

    let from_desc_first = from_desc.trim().lines().next().unwrap_or("(empty)");
    let from_desc_display = if from_desc_first.is_empty() {
        "(empty)"
    } else {
        from_desc_first
    };
    let into_desc_first = into_desc.trim().lines().next().unwrap_or("(empty)");
    let into_desc_display = if into_desc_first.is_empty() {
        "(empty)"
    } else {
        into_desc_first
    };

    let from_short = short_id(from);
    let from_commit_short = short_id(&from_commit_id);
    let into_short = short_id(into);
    let into_commit_short = short_id(&into_commit_id);

    // Enumerate baseline hunks: changes introduced by `from` vs its parent.
    let parent_rev = format!("{from}-");
    let diff_output = jj.diff_from_to_git(&parent_rev, from)?;
    let baseline_hunks = parse_git_diff_hunks(&diff_output);

    // Build REMAINING CHANGES section from all baseline hunks grouped by file.
    let remaining = render_hunk_section(&baseline_hunks);

    let content = format!(
        "SQUASHING:\n\
         From: {from_short} {from_commit_short} {from_desc_display}\n\
         To:   {into_short} {into_commit_short} {into_desc_display}\n\
         \n\
         SELECTED CHANGES:\n\
         \n\
         REMAINING CHANGES:\n\
         {remaining}{}",
        jj.command_reference().squash(),
    );

    let dir = badjuju_dir(workspace)?;
    let squash_dir = dir.join("squash");
    std::fs::create_dir_all(&squash_dir)?;
    let filename = format!("{}-{}.jujutsu", short_id(from), short_id(into));
    let path = squash_dir.join(&filename);
    std::fs::write(&path, &content)?;
    let uri = file_uri(&path);

    let window = SquashWindow {
        from: from.to_string(),
        into: into.to_string(),
        uri: uri.clone(),
        baseline_hunks,
    };
    Ok((uri, window))
}

/// Re-render a squash window after hunks have moved between REMAINING and
/// SELECTED. Computes the current REMAINING from `jj diff --from <from>- --to
/// <from>` and derives SELECTED as `baseline − remaining`. Returns
/// `(uri, new_content)`.
///
/// If `<from>` or `<into>` no longer exist (abandoned externally), returns a
/// "SQUASH TARGET NO LONGER EXISTS" notice instead.
///
/// `write_to_disk` controls whether the rendered content is also written to
/// the squash file on disk. Virtual-diffs-capable clients (VS Code, Neovim)
/// pass `false`: applyEdit alone delivers the new content, and skipping the
/// disk-write avoids triggering Neovim's autoreload — which would re-run the
/// ftplugin and reset user-opened folds. File-based clients (Helix) pass
/// `true` so a cold reopen of the buffer sees fresh content.
pub fn regenerate_squash_window(
    jj: &Jj,
    window: &SquashWindow,
    write_to_disk: bool,
) -> Result<(String, String), CommandError> {
    let from_ok = jj.change_id_of(&window.from).is_ok();
    let into_ok = jj.change_id_of(&window.into).is_ok();

    let path = path_from_uri(&window.uri).ok_or_else(|| {
        CommandError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "bad squash window URI",
        ))
    })?;

    if !from_ok || !into_ok {
        let content = "SQUASH TARGET NO LONGER EXISTS\n\nThe squash source or destination was \
                       abandoned or rewritten externally.\nClose this window.\n"
            .to_string();
        if write_to_disk {
            std::fs::write(&path, &content)?;
        }
        return Ok((window.uri.clone(), content));
    }

    // Remaining = still in <from>
    let parent_rev = format!("{}-", window.from);
    let remaining_diff = jj.diff_from_to_git(&parent_rev, &window.from)?;
    let remaining_hunks = parse_git_diff_hunks(&remaining_diff);

    // Selected = baseline hunks no longer in <from>
    let selected_hunks: Vec<Hunk> = window
        .baseline_hunks
        .iter()
        .filter(|h| {
            !remaining_hunks
                .iter()
                .any(|r| r.file == h.file && r.header == h.header)
        })
        .cloned()
        .collect();

    let from_commit_id = jj.commit_id_of(&window.from)?;
    let into_commit_id = jj.commit_id_of(&window.into)?;
    let from_desc = jj.describe_get(&window.from)?;
    let into_desc = jj.describe_get(&window.into)?;

    let from_desc_first = from_desc.trim().lines().next().unwrap_or("(empty)");
    let from_desc_display = if from_desc_first.is_empty() {
        "(empty)"
    } else {
        from_desc_first
    };
    let into_desc_first = into_desc.trim().lines().next().unwrap_or("(empty)");
    let into_desc_display = if into_desc_first.is_empty() {
        "(empty)"
    } else {
        into_desc_first
    };

    let from_short = short_id(&window.from);
    let from_commit_short = short_id(&from_commit_id);
    let into_short = short_id(&window.into);
    let into_commit_short = short_id(&into_commit_id);

    let selected_section = render_hunk_section(&selected_hunks);
    let remaining_section = render_hunk_section(&remaining_hunks);

    let content = format!(
        "SQUASHING:\n\
         From: {from_short} {from_commit_short} {from_desc_display}\n\
         To:   {into_short} {into_commit_short} {into_desc_display}\n\
         \n\
         SELECTED CHANGES:\n\
         {selected_section}\
         REMAINING CHANGES:\n\
         {remaining_section}\
         {}",
        jj.command_reference().squash(),
    );

    if write_to_disk {
        std::fs::write(&path, &content)?;
    }
    Ok((window.uri.clone(), content))
}

/// Toggle a single hunk between REMAINING and SELECTED. For
/// `SquashSection::Remaining`, squashes the hunk from `<from>` into `<into>`
/// interactively. For `SquashSection::Selected`, squashes it back. Returns the
/// regenerated `(uri, content)` of the squash window.
pub fn run_squash_toggle_hunk(
    jj: &Jj,
    workspace: &Path,
    window: &SquashWindow,
    hunk: &cursor::SquashHunk,
    section: cursor::SquashSection,
    write_to_disk: bool,
) -> Result<(String, String), CommandError> {
    let badjuju_exe = std::env::current_exe().map_err(CommandError::Io)?;
    let sidecar_path = workspace
        .join(".jj")
        .join("badjuju")
        .join("squash_selection.json");

    match section {
        cursor::SquashSection::Remaining => {
            let sel = serde_json::json!({
                "file": hunk.file,
                "hunk_header": hunk.header,
                "hunk_content": hunk.content,
                "direction": "include",
            });
            std::fs::write(&sidecar_path, sel.to_string())?;
            jj.squash_from_into_interactive(
                &window.from,
                &window.into,
                &badjuju_exe,
                &sidecar_path,
            )?;
        }
        cursor::SquashSection::Selected => {
            // Find the hunk as it appears in <into>'s diff (line numbers may differ).
            let into_parent = format!("{}-", window.into);
            let into_diff = jj.diff_from_to_git(&into_parent, &window.into)?;
            let into_hunks = parse_git_diff_hunks(&into_diff);
            let effective = into_hunks
                .iter()
                .find(|h| h.file == hunk.file && hunks_content_match(&h.content, &hunk.content))
                .cloned()
                .unwrap_or_else(|| Hunk {
                    file: hunk.file.clone(),
                    header: hunk.header.clone(),
                    content: hunk.content.clone(),
                });
            let sel = serde_json::json!({
                "file": effective.file,
                "hunk_header": effective.header,
                "hunk_content": effective.content,
                "direction": "include",
            });
            std::fs::write(&sidecar_path, sel.to_string())?;
            jj.squash_from_into_interactive(
                &window.into,
                &window.from,
                &badjuju_exe,
                &sidecar_path,
            )?;
        }
    }

    regenerate_squash_window(jj, window, write_to_disk)
}

/// Toggle an entire file between REMAINING and SELECTED using file-level squash.
pub fn run_squash_toggle_file(
    jj: &Jj,
    window: &SquashWindow,
    file: &str,
    section: cursor::SquashSection,
    write_to_disk: bool,
) -> Result<(String, String), CommandError> {
    match section {
        cursor::SquashSection::Remaining => {
            jj.squash_file_into(&window.from, &window.into, file)?;
        }
        cursor::SquashSection::Selected => {
            jj.squash_file_into(&window.into, &window.from, file)?;
        }
    }
    regenerate_squash_window(jj, window, write_to_disk)
}

/// Move all remaining hunks to SELECTED (`jj squash --from <from> --into <into>`).
pub fn run_squash_select_all(
    jj: &Jj,
    window: &SquashWindow,
    write_to_disk: bool,
) -> Result<(String, String), CommandError> {
    jj.squash_from_into_keep_emptied(&window.from, &window.into)?;
    regenerate_squash_window(jj, window, write_to_disk)
}

/// Move all selected hunks back to REMAINING (`jj squash --from <into> --into <from>`).
pub fn run_squash_select_none(
    jj: &Jj,
    window: &SquashWindow,
    write_to_disk: bool,
) -> Result<(String, String), CommandError> {
    jj.squash_from_into_keep_emptied(&window.into, &window.from)?;
    regenerate_squash_window(jj, window, write_to_disk)
}

/// Compare hunk content lines (ignoring trailing whitespace differences).
fn hunks_content_match(a: &str, b: &str) -> bool {
    a.trim_end() == b.trim_end()
}

// ---------- Hunk-edit buffer (#13) ----------

/// State of an open `hunk-edit.jujutsu` buffer. Stored singly on `State`.
#[derive(Debug, Clone)]
pub enum HunkEdit {
    /// Editing a hunk that will be squashed from `from` into `into`.
    Squash {
        /// File URI of the hunk-edit buffer.
        uri: String,
        /// Full change-id of the squash source.
        from: String,
        /// Full change-id of the squash destination.
        into: String,
        /// File path the hunk applies to.
        file: String,
        /// Original `@@` header — kept so we can preserve `old_start` /
        /// `new_start` and only recompute lengths from the body.
        original_header: String,
        /// Which section the hunk was in before editing — used to decide
        /// whether a "reverse-toggle" unsquash happened first.
        origin_section: cursor::SquashSection,
    },
}

impl HunkEdit {
    pub fn uri(&self) -> &str {
        match self {
            HunkEdit::Squash { uri, .. } => uri,
        }
    }
}

/// Result of saving a hunk-edit buffer.
#[derive(Debug, Clone)]
pub enum HunkEditOutcome {
    /// Hunk was applied successfully. The hunk-edit file is rewritten with a
    /// terminal notice, and the squash window is refreshed.
    Applied {
        window_uri: String,
        window_content: String,
        notice: String,
    },
    /// User cleared the body — no jj invocation, terminal notice rendered.
    Aborted { notice: String },
    /// `<from>` was abandoned externally — no jj invocation, terminal notice.
    StaleSource { notice: String },
}

const HUNK_EDIT_FILENAME: &str = "hunk-edit.jujutsu";
const HUNK_EDIT_APPLIED_NOTICE: &str = "EDIT APPLIED — close this buffer\n";
const HUNK_EDIT_ABORTED_NOTICE: &str = "EDIT ABORTED — close this buffer\n";
const HUNK_EDIT_STALE_NOTICE: &str = "STALE SOURCE — close this buffer\n";

/// Build the JJ:-prefixed metadata + body for a hunk-edit buffer.
fn render_hunk_edit_buffer(
    action: &str,
    from: &str,
    into: &str,
    file: &str,
    original_header: &str,
    hunk_body: &str,
    command_reference: &str,
) -> String {
    let mut out = String::new();
    out.push_str("JJ: Edit the +/- lines below, then save to apply.\n");
    out.push_str("JJ: Editing `-` line text has no effect; editing ` ` (context) text\n");
    out.push_str("JJ: replaces the source. Add/remove only `+` lines for safe edits.\n");
    out.push_str(&format!("JJ: action: {action}\n"));
    out.push_str(&format!("JJ: from: {from}\n"));
    out.push_str(&format!("JJ: into: {into}\n"));
    out.push_str(&format!("JJ: file: {file}\n"));
    out.push_str(&format!("JJ: original-header: {original_header}\n"));
    out.push('\n');
    out.push_str(file);
    out.push('\n');
    out.push_str(original_header);
    out.push('\n');
    out.push_str(hunk_body);
    if !hunk_body.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(command_reference);
    out
}

/// Strip leading `JJ:`-prefix lines from `text` and return the `(header, body)`
/// of the user-edited hunk. Returns `None` when there is no `@@` line or the
/// body (the `+/-/space` lines after `@@`) is empty.
pub fn parse_hunk_edit_buffer(text: &str) -> Option<(String, String)> {
    // Skip JJ: lines and the file-path header line; find the @@ header.
    let mut header: Option<String> = None;
    let mut body_lines: Vec<&str> = Vec::new();
    let mut in_body = false;
    for line in text.lines() {
        if line.starts_with("JJ:") {
            continue;
        }
        if line.starts_with("COMMAND REFERENCE:") {
            break;
        }
        if !in_body {
            if line.starts_with("@@") {
                header = Some(line.to_string());
                in_body = true;
            }
            continue;
        }
        // Inside body: collect only `+/-/space` lines. Anything else ends the
        // body (a stray COMMAND REFERENCE block, blank tail, etc.).
        if line.starts_with('+') || line.starts_with('-') || line.starts_with(' ') {
            body_lines.push(line);
        } else {
            break;
        }
    }
    let header = header?;
    if body_lines.is_empty() {
        return None;
    }
    Some((header, body_lines.join("\n")))
}

/// Recompute an `@@` hunk header from the body lines: keep `old_start` and
/// `new_start` from the original, recount `old_len` (` ` + `-` lines) and
/// `new_len` (` ` + `+` lines) from the body. Rejects unknown line prefixes.
pub fn recompute_hunk_header(original: &str, body: &str) -> Result<String, CommandError> {
    let (old_start, _) = crate::squash_tool::parse_hunk_old_range(original).ok_or_else(|| {
        CommandError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("could not parse @@ header: {original:?}"),
        ))
    })?;
    let new_start = parse_hunk_new_start(original).ok_or_else(|| {
        CommandError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("could not parse +N from @@ header: {original:?}"),
        ))
    })?;

    let mut old_len = 0usize;
    let mut new_len = 0usize;
    for line in body.lines() {
        if let Some(first) = line.chars().next() {
            match first {
                ' ' => {
                    old_len += 1;
                    new_len += 1;
                }
                '-' => old_len += 1,
                '+' => new_len += 1,
                _ => {
                    return Err(CommandError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("unknown hunk-body line prefix: {line:?}"),
                    )));
                }
            }
        }
    }

    Ok(format!(
        "@@ -{old_start},{old_len} +{new_start},{new_len} @@"
    ))
}

/// Parse `+N` or `+N,M` (the new-range portion of an `@@` header) and return
/// `N` (the new start line, 1-indexed in the diff format).
fn parse_hunk_new_start(header: &str) -> Option<usize> {
    let plus_idx = header.find('+')?;
    let rest = &header[plus_idx + 1..];
    let end = rest.find([' ', ',', '@'])?;
    let start_str = &rest[..end];
    start_str.parse::<usize>().ok()
}

/// Open the hunk-edit buffer for the hunk at the cursor inside a squash
/// window. For SELECTED hunks, first reverse-toggle the hunk back to REMAINING
/// (so the user edits in the source revision), then write the hunk-edit file.
///
/// Returns the hunk-edit URI, the `HunkEdit` state to persist, and an
/// optional `(squash_uri, squash_content)` to push back to the client if the
/// reverse-toggle ran.
#[allow(clippy::type_complexity)]
pub fn run_squash_open_hunk_edit(
    jj: &Jj,
    workspace: &Path,
    window: &SquashWindow,
    hunk: &cursor::SquashHunk,
    section: cursor::SquashSection,
    write_to_disk: bool,
) -> Result<(String, HunkEdit, Option<(String, String)>), CommandError> {
    let mut window_update: Option<(String, String)> = None;
    if section == cursor::SquashSection::Selected {
        // Unsquash the hunk from <into> back into <from> first.
        let (uri, content) = run_squash_toggle_hunk(
            jj,
            workspace,
            window,
            hunk,
            cursor::SquashSection::Selected,
            write_to_disk,
        )?;
        window_update = Some((uri, content));
    }

    let dir = badjuju_dir(workspace)?;
    let path = dir.join(HUNK_EDIT_FILENAME);
    let uri = file_uri(&path);

    let content = render_hunk_edit_buffer(
        "squash",
        &window.from,
        &window.into,
        &hunk.file,
        &hunk.header,
        &hunk.content,
        jj.command_reference().hunk_edit(),
    );
    std::fs::write(&path, &content)?;

    let edit = HunkEdit::Squash {
        uri: uri.clone(),
        from: window.from.clone(),
        into: window.into.clone(),
        file: hunk.file.clone(),
        original_header: hunk.header.clone(),
        origin_section: section,
    };
    Ok((uri, edit, window_update))
}

/// Handle a save on `hunk-edit.jujutsu`: parse the buffer, validate, run
/// `jj squash --interactive --tool`, regenerate the squash window, and rewrite
/// the hunk-edit file with a terminal notice. Caller persists/clears state.
pub fn on_hunk_edit_save(
    jj: &Jj,
    workspace: &Path,
    edit: &HunkEdit,
    text: &str,
    write_to_disk: bool,
) -> Result<HunkEditOutcome, CommandError> {
    let HunkEdit::Squash {
        uri,
        from,
        into,
        file,
        original_header,
        ..
    } = edit;

    // Empty body / no @@ → abort cleanly.
    let Some((_, body)) = parse_hunk_edit_buffer(text) else {
        let notice = HUNK_EDIT_ABORTED_NOTICE.to_string();
        if let Some(path) = path_from_uri(uri) {
            let _ = std::fs::write(&path, &notice);
        }
        return Ok(HunkEditOutcome::Aborted { notice });
    };

    // Source vanished externally → stale notice, no jj call.
    if jj.change_id_of(from).is_err() {
        let notice = HUNK_EDIT_STALE_NOTICE.to_string();
        if let Some(path) = path_from_uri(uri) {
            let _ = std::fs::write(&path, &notice);
        }
        return Ok(HunkEditOutcome::StaleSource { notice });
    }

    let new_header = recompute_hunk_header(original_header, &body)?;

    // Stage the sidecar and run interactive squash from <from> into <into>.
    let badjuju_exe = std::env::current_exe().map_err(CommandError::Io)?;
    let sidecar_path = workspace
        .join(".jj")
        .join("badjuju")
        .join("squash_selection.json");
    let sel = serde_json::json!({
        "file": file,
        "hunk_header": new_header,
        "hunk_content": body,
        "direction": "include",
    });
    std::fs::write(&sidecar_path, sel.to_string())?;
    jj.squash_from_into_interactive(from, into, &badjuju_exe, &sidecar_path)?;

    // Regenerate the squash window the user came from.
    let baseline_window = SquashWindow {
        from: from.clone(),
        into: into.clone(),
        uri: squash_window_uri_for(workspace, from, into)?,
        baseline_hunks: Vec::new(),
    };
    let (window_uri, window_content) =
        regenerate_squash_window(jj, &baseline_window, write_to_disk)?;

    // Replace the hunk-edit file with the terminal notice.
    let notice = HUNK_EDIT_APPLIED_NOTICE.to_string();
    if let Some(path) = path_from_uri(uri) {
        let _ = std::fs::write(&path, &notice);
    }

    Ok(HunkEditOutcome::Applied {
        window_uri,
        window_content,
        notice,
    })
}

/// Build the on-disk URI for a squash window between `from` and `into`,
/// matching the filename convention in [`run_squash_window`].
fn squash_window_uri_for(workspace: &Path, from: &str, into: &str) -> Result<String, CommandError> {
    let dir = badjuju_dir(workspace)?;
    let squash_dir = dir.join("squash");
    let filename = format!("{}-{}.jujutsu", short_id(from), short_id(into));
    Ok(file_uri(&squash_dir.join(&filename)))
}

/// Render baseline hunks as plain-text grouped by file path, ready to embed
/// in a REMAINING CHANGES section. Files are separated by a blank line.
fn render_hunk_section(hunks: &[Hunk]) -> String {
    if hunks.is_empty() {
        return "\n".to_string();
    }
    let mut out = String::new();
    let mut last_file: Option<&str> = None;
    for hunk in hunks {
        if last_file != Some(hunk.file.as_str()) {
            if last_file.is_some() {
                out.push('\n');
            }
            out.push('\n');
            out.push_str(&hunk.file);
            last_file = Some(&hunk.file);
        }
        out.push('\n');
        out.push_str(&hunk.header);
        if !hunk.content.is_empty() {
            out.push('\n');
            out.push_str(&hunk.content);
        }
    }
    out.push_str("\n\n");
    out
}

/// Return folding ranges for a squash window buffer.
///
/// Two levels: file → `@@` hunk. Section headers (SELECTED/REMAINING) are
/// not themselves folded so they stay visible when files are collapsed.
pub fn squash_folding_ranges(content: &str) -> Vec<FoldingRange> {
    let lines: Vec<&str> = content.lines().collect();
    let mut ranges = Vec::new();

    let cmd_ref_line = lines
        .iter()
        .position(|l| l.starts_with("COMMAND REFERENCE:"))
        .unwrap_or(lines.len());

    let selected_line = lines.iter().position(|l| *l == "SELECTED CHANGES:");
    let remaining_line = lines.iter().position(|l| *l == "REMAINING CHANGES:");

    for (section_start, section_end) in [
        (selected_line, remaining_line.unwrap_or(cmd_ref_line)),
        (remaining_line, cmd_ref_line),
    ] {
        let Some(ss) = section_start else { continue };
        squash_section_file_hunk_folds(&lines, ss + 1, section_end, &mut ranges);
    }
    ranges
}

/// Emit file and hunk folds for the content within a squash section (SELECTED or
/// REMAINING CHANGES). `start` is the line after the section header; `end` is
/// the first line of the next section (exclusive).
fn squash_section_file_hunk_folds(
    lines: &[&str],
    start: usize,
    end: usize,
    ranges: &mut Vec<FoldingRange>,
) {
    let mut file_start: Option<usize> = None;
    let mut hunk_start: Option<usize> = None;
    let mut last_hunk_content: Option<usize> = None;
    let mut last_file_content: Option<usize> = None;

    let flush_hunk =
        |hs: &mut Option<usize>, hc: &mut Option<usize>, ranges: &mut Vec<FoldingRange>| {
            if let (Some(h), Some(c)) = (*hs, *hc)
                && c > h
            {
                ranges.push(make_region(h, c));
            }
            *hs = None;
            *hc = None;
        };
    let flush_file =
        |fs: &mut Option<usize>, fc: &mut Option<usize>, ranges: &mut Vec<FoldingRange>| {
            if let (Some(f), Some(c)) = (*fs, *fc)
                && c > f
            {
                ranges.push(make_region(f, c));
            }
            *fs = None;
            *fc = None;
        };

    for (i, line) in lines.iter().enumerate().take(end).skip(start) {
        if line.is_empty() {
            flush_hunk(&mut hunk_start, &mut last_hunk_content, ranges);
            flush_file(&mut file_start, &mut last_file_content, ranges);
        } else if line.starts_with("@@") {
            flush_hunk(&mut hunk_start, &mut last_hunk_content, ranges);
            hunk_start = Some(i);
            last_file_content = Some(i);
        } else if hunk_start.is_some()
            && (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
        {
            last_hunk_content = Some(i);
            last_file_content = Some(i);
        } else if !line.starts_with(' ') && !line.starts_with('+') && !line.starts_with('-') {
            // Plain file path line.
            flush_hunk(&mut hunk_start, &mut last_hunk_content, ranges);
            flush_file(&mut file_start, &mut last_file_content, ranges);
            file_start = Some(i);
        }
    }
    flush_hunk(&mut hunk_start, &mut last_hunk_content, ranges);
    flush_file(&mut file_start, &mut last_file_content, ranges);
}

/// Which flavor of diff buffer was opened. Determines filename and refresh policy.
#[derive(Debug, Clone)]
pub enum DiffTarget {
    /// Tracks a mutable change (identified by full change-id). Re-rendered
    /// after every state-changing jj operation so the view stays current.
    Change(String),
    /// Pinned to an immutable commit (identified by full commit-id). Never
    /// refreshed — commits are immutable by definition.
    Commit(String),
}

/// First 12 characters of a full id, used for human-readable diff filenames.
fn short_id(full: &str) -> &str {
    &full[..full.len().min(12)]
}

/// Run `badjuju.diff` (change mode): write `diff-change-<id>.jujutsu` showing
/// `jj diff -r <change-id>`. The revision is resolved to a stable change-id so
/// the filename is stable across amends of the same change. Embeds a
/// `CHANGE_ID:` header for refresh and code-action resolution.
///
/// Returns `(file_uri, DiffTarget::Change(<full_change_id>))`.
pub fn run_diff_change(
    jj: &Jj,
    workspace: &Path,
    revision: &str,
) -> Result<(String, DiffTarget), CommandError> {
    run_diff_change_with_content(jj, workspace, revision).map(|(uri, target, _)| (uri, target))
}

/// Same as [`run_diff_change`], but additionally returns the content written
/// to disk so callers can ship it to clients without re-reading the file.
pub fn run_diff_change_with_content(
    jj: &Jj,
    workspace: &Path,
    revision: &str,
) -> Result<(String, DiffTarget, String), CommandError> {
    let rev = revision_or_at(revision);
    let change_id = jj.change_id_of(rev)?;
    let content = diff_content_for_change(jj, &change_id)?;
    let dir = badjuju_dir(workspace)?;
    let path = dir.join(format!("diff-change-{}.jujutsu", short_id(&change_id)));
    std::fs::write(&path, &content)?;
    Ok((file_uri(&path), DiffTarget::Change(change_id), content))
}

/// Run `badjuju.diff.commit` (commit mode): write `diff-commit-<id>.jujutsu`
/// pinned to the exact commit-id at call time. The file is never refreshed.
///
/// Returns `(file_uri, DiffTarget::Commit(<full_commit_id>))`.
pub fn run_diff_commit(
    jj: &Jj,
    workspace: &Path,
    revision: &str,
) -> Result<(String, DiffTarget), CommandError> {
    let rev = revision_or_at(revision);
    let commit_id = jj.commit_id_of(rev)?;
    let output = jj.diff(&commit_id)?;
    let content = format!(
        "COMMIT_ID: {}\n\nDIFF:\n\n{}\n\n{}",
        commit_id,
        output.trim_end(),
        jj.command_reference().diff(),
    );
    let dir = badjuju_dir(workspace)?;
    let path = dir.join(format!("diff-commit-{}.jujutsu", short_id(&commit_id)));
    std::fs::write(&path, &content)?;
    Ok((file_uri(&path), DiffTarget::Commit(commit_id)))
}

/// Return a virtual `badjuju-diff:///change/<id>` URI for a change-mode diff
/// without writing anything to disk. Used by capable clients (VS Code, Neovim
/// with polyfill) that implement `workspace/textDocumentContent`.
pub fn run_diff_change_virtual(
    jj: &Jj,
    revision: &str,
) -> Result<(String, DiffTarget), CommandError> {
    let rev = revision_or_at(revision);
    let change_id = jj.change_id_of(rev)?;
    let uri = format!("badjuju-diff:///change/{}", change_id);
    Ok((uri, DiffTarget::Change(change_id)))
}

/// Return a virtual `badjuju-diff:///commit/<id>` URI for a commit-mode diff
/// without writing anything to disk.
pub fn run_diff_commit_virtual(
    jj: &Jj,
    revision: &str,
) -> Result<(String, DiffTarget), CommandError> {
    let rev = revision_or_at(revision);
    let commit_id = jj.commit_id_of(rev)?;
    let uri = format!("badjuju-diff:///commit/{}", commit_id);
    Ok((uri, DiffTarget::Commit(commit_id)))
}

/// Generate the text content of a change-mode diff without writing to disk.
/// Used by the `workspace/textDocumentContent` handler for `badjuju-diff:` URIs.
pub fn diff_content_for_change(jj: &Jj, change_id: &str) -> Result<String, CommandError> {
    let output = jj.diff(change_id)?;
    Ok(format!(
        "CHANGE_ID: {}\n\nDIFF:\n\n{}\n\n{}",
        change_id,
        output.trim_end(),
        jj.command_reference().diff(),
    ))
}

/// Generate the text content of a commit-mode diff without writing to disk.
/// Used by the `workspace/textDocumentContent` handler for `badjuju-diff:` URIs.
pub fn diff_content_for_commit(jj: &Jj, commit_id: &str) -> Result<String, CommandError> {
    let output = jj.diff(commit_id)?;
    Ok(format!(
        "COMMIT_ID: {}\n\nDIFF:\n\n{}\n\n{}",
        commit_id,
        output.trim_end(),
        jj.command_reference().diff(),
    ))
}

/// Resolve `revision` to a commit-id and return the virtual file-blob URI
/// `badjuju-file:///commit/<commit-id>/<path>`. No disk write — virtual-
/// capable clients (VS Code, Neovim) fetch the content via
/// `workspace/textDocumentContent` and route it through
/// [`file_content_at_commit`].
pub fn file_blob_uri_virtual(jj: &Jj, revision: &str, path: &str) -> Result<String, CommandError> {
    let commit_id = jj.commit_id_of(revision)?;
    Ok(format!("badjuju-file:///commit/{commit_id}/{path}"))
}

/// Materialize the file's content at `revision` on disk for file-only clients
/// (Helix). Writes to `.jj/badjuju/blobs/<hash>/<basename>` where:
/// - `<hash>` is a 16-char hex digest of `(commit_id, path)` so distinct
///   pairs land in distinct directories, and a re-fetch of the same pair
///   reuses the same path.
/// - `<basename>` preserves the source file's name (and therefore its
///   extension) so editors infer language correctly.
///
/// Returns the resulting `file://` URI.
pub fn file_blob_with_path(
    jj: &Jj,
    workspace: &Path,
    revision: &str,
    path: &str,
) -> Result<String, CommandError> {
    let commit_id = jj.commit_id_of(revision)?;
    let content = jj.file_show(path, &commit_id)?;
    let hash = blob_hash(&commit_id, path);
    let basename = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    let dir = badjuju_dir(workspace)?;
    let blob_dir = dir.join("blobs").join(&hash);
    std::fs::create_dir_all(&blob_dir)?;
    let file_path = blob_dir.join(&basename);
    std::fs::write(&file_path, &content)?;
    Ok(file_uri(&file_path))
}

/// Read a file's content at a specific commit-id. Used by the
/// `workspace/textDocumentContent` handler for `badjuju-file://` URIs.
pub fn file_content_at_commit(
    jj: &Jj,
    commit_id: &str,
    path: &str,
) -> Result<String, CommandError> {
    Ok(jj.file_show(path, commit_id)?)
}

/// Deterministic 16-char hex hash of `(commit_id, path)`. Uses the std
/// `DefaultHasher` (SipHash-1-3 with fixed keys) — sufficient to keep
/// distinct `(commit, path)` pairs in distinct directories without
/// pulling in a crypto dep. Not security-sensitive.
fn blob_hash(commit_id: &str, path: &str) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    commit_id.hash(&mut h);
    0u8.hash(&mut h);
    path.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Extract the change-id encoded in the filename of a `diff-change-*.jujutsu`
/// URI. Used by `did_open` auto-populate to re-run the diff for the right
/// change when a user manually opens an existing diff file.
pub fn parse_change_id_from_uri(uri: &str) -> Option<String> {
    let name = uri.rsplit('/').next()?;
    let after = name.strip_prefix("diff-change-")?;
    let id = after.strip_suffix(".jujutsu")?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Extract the commit-id encoded in the filename of a `diff-commit-*.jujutsu`
/// URI.
pub fn parse_commit_id_from_uri(uri: &str) -> Option<String> {
    let name = uri.rsplit('/').next()?;
    let after = name.strip_prefix("diff-commit-")?;
    let id = after.strip_suffix(".jujutsu")?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

/// Re-render all open change-mode diff buffers. Called after any state-changing
/// jj operation. Returns the `(uri, content)` of each successfully refreshed
/// change diff so callers can deliver the new content to file-based clients
/// via `workspace/applyEdit` without re-reading from disk. Errors per diff are
/// silently dropped so one stale diff doesn't block the others from refreshing.
pub fn refresh_change_diffs(
    jj: &Jj,
    workspace: &Path,
    open_diffs: &HashMap<String, DiffTarget>,
) -> Vec<(String, String)> {
    let mut refreshed = Vec::new();
    for target in open_diffs.values() {
        if let DiffTarget::Change(change_id) = target
            && let Ok((uri, _, content)) = run_diff_change_with_content(jj, workspace, change_id)
        {
            refreshed.push((uri, content));
        }
    }
    refreshed
}

/// Remove stale `diff-change-*.jujutsu` and `diff-commit-*.jujutsu` files left
/// over from a previous server session. Called during `initialize` before the
/// server starts tracking new open diffs.
pub fn sweep_stale_diff_files(workspace: &Path) {
    let Ok(dir) = badjuju_dir(workspace) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if (name.starts_with("diff-change-") || name.starts_with("diff-commit-"))
            && name.ends_with(".jujutsu")
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// Extract the revision from the `REVISION:` header of the legacy diff.jujutsu
/// format. Kept for backward-compatibility with the refresh path.
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
/// For diff-change-*.jujutsu → re-run change diff. For diff-commit-*.jujutsu → re-run commit diff.
/// Falls back to status when the URI doesn't decode to a known badjuju buffer.
pub fn run_refresh(jj: &Jj, workspace: &Path, uri: &str) -> Result<String, CommandError> {
    let Some(path) = path_from_uri(uri) else {
        return run_status(jj, workspace);
    };
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    if filename == "log.jujutsu" {
        let content = std::fs::read_to_string(&path)?;
        let revset = parse_log_revset(&content).unwrap_or_else(|| "@".to_string());
        return run_log(jj, workspace, &revset);
    }

    if filename.starts_with("diff-change-") && filename.ends_with(".jujutsu") {
        let id = filename
            .strip_prefix("diff-change-")
            .and_then(|s| s.strip_suffix(".jujutsu"))
            .unwrap_or("@");
        return run_diff_change(jj, workspace, id).map(|(uri, _)| uri);
    }

    if filename.starts_with("diff-commit-") && filename.ends_with(".jujutsu") {
        let id = filename
            .strip_prefix("diff-commit-")
            .and_then(|s| s.strip_suffix(".jujutsu"))
            .unwrap_or("@");
        return run_diff_commit(jj, workspace, id).map(|(uri, _)| uri);
    }

    if filename == "diff.jujutsu" {
        // Legacy format: re-run using the REVISION: header value.
        let content = std::fs::read_to_string(&path)?;
        let revision = parse_diff_revision(&content).unwrap_or_else(|| "@".to_string());
        return run_diff_change(jj, workspace, &revision).map(|(uri, _)| uri);
    }

    run_status(jj, workspace)
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
    match jj.next_change(edit) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => {
            let label = if edit { "next --edit" } else { "next" };
            write_status(jj, workspace, Some(&format!("{label} failed: {e}")))
        }
    }
}

/// Run `badjuju.prev`: move the working copy to an ancestor revision (`jj prev`),
/// optionally with `--edit`. On failure, surface the error as a MESSAGE prelude
/// in the status buffer.
pub fn run_prev(jj: &Jj, workspace: &Path, edit: bool) -> Result<String, CommandError> {
    match jj.prev_change(edit) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => {
            let label = if edit { "prev --edit" } else { "prev" };
            write_status(jj, workspace, Some(&format!("{label} failed: {e}")))
        }
    }
}

/// Run `badjuju.undo`: revert the last operation with `jj undo`, then refresh status.
/// Surfaces failures as a MESSAGE: prelude in the status buffer.
pub fn run_undo(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    match jj.undo() {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(jj, workspace, Some(&format!("undo failed: {e}"))),
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
    if dest.is_empty() {
        return write_status(jj, workspace, Some("rebase: destination revision required"));
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
    match jj.git_push() {
        Ok(_) => run_status(jj, workspace),
        Err(e) => write_status(jj, workspace, Some(&format!("push failed: {e}"))),
    }
}

/// Run `badjuju.fetch`: run `jj git fetch`, then refresh status.
/// Surfaces failures as a MESSAGE prelude.
pub fn run_fetch(jj: &Jj, workspace: &Path) -> Result<String, CommandError> {
    match jj.git_fetch() {
        Ok(_) => run_status(jj, workspace),
        Err(e) => write_status(jj, workspace, Some(&format!("fetch failed: {e}"))),
    }
}

/// Run `badjuju.edit`: move @ to `revision` (`jj edit REV`), then refresh status
/// and log (if log file exists). Surfaces failures as a MESSAGE prelude.
pub fn run_edit(jj: &Jj, workspace: &Path, revision: &str) -> Result<String, CommandError> {
    let rev = revision_or_at(revision);
    match jj.edit(rev) {
        Ok(()) => {
            regenerate_log_if_present(jj, workspace)?;
            run_status(jj, workspace)
        }
        Err(e) => write_status(jj, workspace, Some(&format!("edit {rev} failed: {e}"))),
    }
}

/// Run `badjuju.abandon`: abandon `revision` (defaults to `@`) and refresh status.
/// Surfaces failures as a MESSAGE: prelude in the status buffer.
pub fn run_abandon(jj: &Jj, workspace: &Path, revision: &str) -> Result<String, CommandError> {
    let rev = revision_or_at(revision);
    match jj.abandon(rev) {
        Ok(()) => run_status(jj, workspace),
        Err(e) => write_status(jj, workspace, Some(&format!("abandon {rev} failed: {e}"))),
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
    if name.is_empty() {
        return write_status(jj, workspace, Some("bookmark: name is required"));
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
pub fn regenerate_log_if_present(jj: &Jj, workspace: &Path) -> Result<(), CommandError> {
    regenerate_log_if_present_with_content(jj, workspace).map(|_| ())
}

/// Same as [`regenerate_log_if_present`], but returns `Some((uri, content))`
/// when the file was regenerated and `None` when the file was absent so
/// callers can deliver the new content to file-based clients.
pub fn regenerate_log_if_present_with_content(
    jj: &Jj,
    workspace: &Path,
) -> Result<Option<(String, String)>, CommandError> {
    let log_path = workspace.join(".jj").join("badjuju").join("log.jujutsu");
    if !log_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&log_path)?;
    let revset = parse_log_revset(&content).unwrap_or_else(|| DEFAULT_LOG_REVSET.to_string());
    let result = run_log_with_content(jj, workspace, &revset)?;
    Ok(Some(result))
}

/// On log.jujutsu save: re-parse the REVSET: header and regenerate the file.
pub fn on_log_save(jj: &Jj, workspace: &Path, content: &str) -> Result<String, CommandError> {
    let revset = parse_log_revset(content).unwrap_or_else(|| "@".to_string());
    run_log(jj, workspace, &revset)
}

/// Return folding ranges for a status.jujutsu buffer.
///
/// Emits three levels of nested ranges for CHANGES sections (section ⊃ file ⊃ hunk)
/// plus one range per commit in the STACK section. All ranges use
/// `FoldingRangeKind::Region`.
pub fn status_folding_ranges(content: &str) -> Vec<FoldingRange> {
    let lines: Vec<&str> = content.lines().collect();
    let mut ranges = Vec::new();

    let Some(stack_start) = lines.iter().position(|l| l.starts_with("STACK:")) else {
        return ranges;
    };

    // --- CHANGES section folds (before STACK) ---
    changes_folding_ranges(&lines, stack_start, &mut ranges);

    // --- STACK per-commit folds ---
    let stack_end = lines[stack_start..]
        .iter()
        .position(|l| l.starts_with("COMMAND REFERENCE:"))
        .map(|i| stack_start + i)
        .unwrap_or(lines.len());

    let mut current_header: Option<usize> = None;
    let mut last_nonempty: Option<usize> = None;

    for (i, &line) in lines
        .iter()
        .enumerate()
        .take(stack_end)
        .skip(stack_start + 1)
    {
        let is_commit = cursor::match_commit_header(line).is_some();
        let is_section = line.starts_with("COMMAND REFERENCE:") || line.starts_with("STACK:");

        if is_commit || is_section {
            if let (Some(header), Some(last)) = (current_header, last_nonempty)
                && last > header
            {
                ranges.push(make_region(header, last));
            }
            current_header = if is_commit { Some(i) } else { None };
            last_nonempty = None;
        } else if !line.trim().is_empty() && current_header.is_some() {
            last_nonempty = Some(i);
        }
    }

    if let (Some(header), Some(last)) = (current_header, last_nonempty)
        && last > header
    {
        ranges.push(make_region(header, last));
    }

    ranges
}

fn make_region(start: usize, end: usize) -> FoldingRange {
    FoldingRange {
        start_line: start as u32,
        end_line: end as u32,
        start_character: None,
        end_character: None,
        kind: Some(tower_lsp::lsp_types::FoldingRangeKind::Region),
        collapsed_text: None,
    }
}

/// Emit nested folding ranges for WORKING COPY CHANGES / PARENT CHANGES sections
/// that appear before the STACK line. Three levels: section ⊃ file ⊃ hunk.
fn changes_folding_ranges(lines: &[&str], stack_start: usize, ranges: &mut Vec<FoldingRange>) {
    let mut section_start: Option<usize> = None;
    let mut file_start: Option<usize> = None;
    let mut hunk_start: Option<usize> = None;
    let mut last_hunk_content: Option<usize> = None;
    let mut last_file_content: Option<usize> = None;
    let mut last_section_content: Option<usize> = None;

    let flush_hunk = |hunk_start: &mut Option<usize>,
                      last_hunk_content: &mut Option<usize>,
                      ranges: &mut Vec<FoldingRange>| {
        if let (Some(hs), Some(hc)) = (*hunk_start, *last_hunk_content)
            && hc > hs
        {
            ranges.push(make_region(hs, hc));
        }
        *hunk_start = None;
        *last_hunk_content = None;
    };

    let flush_file = |file_start: &mut Option<usize>,
                      last_file_content: &mut Option<usize>,
                      ranges: &mut Vec<FoldingRange>| {
        if let (Some(fs), Some(fc)) = (*file_start, *last_file_content)
            && fc > fs
        {
            ranges.push(make_region(fs, fc));
        }
        *file_start = None;
        *last_file_content = None;
    };

    let flush_section = |section_start: &mut Option<usize>,
                         last_section_content: &mut Option<usize>,
                         ranges: &mut Vec<FoldingRange>| {
        if let (Some(ss), Some(sc)) = (*section_start, *last_section_content)
            && sc > ss
        {
            ranges.push(make_region(ss, sc));
        }
        *section_start = None;
        *last_section_content = None;
    };

    for (i, &line) in lines.iter().enumerate().take(stack_start) {
        if line.starts_with("WORKING COPY CHANGES (") || line.starts_with("PARENT CHANGES (") {
            flush_hunk(&mut hunk_start, &mut last_hunk_content, ranges);
            flush_file(&mut file_start, &mut last_file_content, ranges);
            flush_section(&mut section_start, &mut last_section_content, ranges);
            section_start = Some(i);
        } else if line.is_empty() {
            flush_hunk(&mut hunk_start, &mut last_hunk_content, ranges);
            flush_file(&mut file_start, &mut last_file_content, ranges);
            flush_section(&mut section_start, &mut last_section_content, ranges);
        } else if line.starts_with("@@") {
            flush_hunk(&mut hunk_start, &mut last_hunk_content, ranges);
            hunk_start = Some(i);
            last_file_content = Some(i);
            last_section_content = Some(i);
        } else if hunk_start.is_some()
            && (line.starts_with('+') || line.starts_with('-') || line.starts_with(' '))
        {
            last_hunk_content = Some(i);
            last_file_content = Some(i);
            last_section_content = Some(i);
        } else if section_start.is_some() {
            // Plain flush-left file path line inside a CHANGES section.
            flush_hunk(&mut hunk_start, &mut last_hunk_content, ranges);
            flush_file(&mut file_start, &mut last_file_content, ranges);
            file_start = Some(i);
            last_section_content = Some(i);
        }
    }

    // Flush anything not yet closed when STACK: is reached.
    flush_hunk(&mut hunk_start, &mut last_hunk_content, ranges);
    flush_file(&mut file_start, &mut last_file_content, ranges);
    flush_section(&mut section_start, &mut last_section_content, ranges);
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("jj error: {0}")]
    Jj(#[from] JjError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Squash from @ into one of multiple parents is ambiguous; the client
    /// must prompt for the target parent and retry with `badjuju.squash.into`.
    #[error("squash requires parent selection")]
    RequiresParentSelection {
        file: String,
        /// `(change_id, short_description)` pairs for all candidate parents.
        candidates: Vec<(String, String)>,
    },
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
        assert!(content.contains("@  :"), "missing @   header:\n{content}");
        assert!(content.contains("@- :"), "missing @-  header:\n{content}");
        assert!(content.contains("STACK:"));
        assert!(content.contains("COMMAND REFERENCE:"));
    }

    #[test]
    fn write_status_emits_at_and_parent_header_lines() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "my work").unwrap();
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.lines().any(|l| l.starts_with("@  :")),
            "missing @   header line:\n{content}"
        );
        assert!(
            content.lines().any(|l| l.starts_with("@- :")),
            "missing @-  header line:\n{content}"
        );
        assert!(
            content.contains("my work"),
            "description should appear in @   header:\n{content}"
        );
    }

    #[test]
    fn write_status_brackets_bookmarks_on_header_line() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        jj.describe_set("@", "parent").unwrap();
        jj.bookmark_create("mymark", "@").unwrap();
        jj.new_change("").unwrap();
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.contains("<mymark>"),
            "expected bookmark <mymark> in header:\n{content}"
        );
    }

    #[test]
    fn write_status_handles_merge_with_multiple_parent_lines() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Create two branches and merge them.
        jj.describe_set("@", "branch-a").unwrap();
        let a_id = jj.change_ids("@").unwrap().first().cloned().unwrap();
        jj.new_change("").unwrap();
        jj.describe_set("@", "branch-b").unwrap();
        let b_id = jj.change_ids("@").unwrap().first().cloned().unwrap();
        // Merge: jj new with two parents
        std::process::Command::new("jj")
            .args(["new", &a_id, &b_id])
            .current_dir(dir.path())
            .output()
            .expect("jj new (merge) failed");
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        let at_minus_lines: Vec<_> = content.lines().filter(|l| l.starts_with("@- :")).collect();
        assert_eq!(
            at_minus_lines.len(),
            2,
            "expected two @-  lines for a merge commit; got:\n{content}"
        );
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
            "n", "L", "r", "e", "d", "D", "s", "U", "a", "f", "p", "P", "u", "R", "q", "?",
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
        assert_eq!(
            default.squash(),
            render_command_reference(&KeymapProfile::Magit, "squash")
        );
    }

    #[test]
    fn command_reference_override_passes_through_each_buffer() {
        let dir = tempdir().unwrap();
        let reference = CommandReference::new(
            Some("CUSTOM STATUS REF".to_string()),
            Some("CUSTOM LOG REF".to_string()),
            Some("CUSTOM DIFF REF".to_string()),
            None,
            None,
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

        let (diff_uri, _) = run_diff_change(&jj, dir.path(), "@").unwrap();
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
        let reference = CommandReference::new(
            None,
            Some("LOG ONLY OVERRIDE".to_string()),
            None,
            None,
            None,
        );
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
    fn run_log_empty_revset_defaults_to_mutable() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_log(&jj, dir.path(), "").expect("run_log failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(
            content.starts_with("REVSET: ancestors(mutable(), 2)"),
            "empty revset should default to ancestors(mutable(), 2):\n{content}"
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
            content.contains("ancestors(mutable(), 2)"),
            "missing Mutable revset:\n{content}"
        );
        assert!(
            content.contains("JJ: Slice:"),
            "missing Slice shortcut:\n{content}"
        );
        assert!(
            content.contains("ancestors(reachable(@, mutable()), 2)"),
            "missing Slice revset:\n{content}"
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
        let slice_line_idx = content
            .lines()
            .position(|l| l.starts_with("JJ: Slice:"))
            .expect("Slice shortcut line not found");
        assert!(
            mutable_line_idx > revset_line_idx,
            "Mutable shortcut should appear after REVSET line"
        );
        assert!(
            slice_line_idx > mutable_line_idx,
            "Slice shortcut should appear after Mutable"
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
        assert!(content.contains("@  :"));
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
    fn run_diff_change_writes_file_with_change_id_header() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "hello\n").unwrap();
        jj.describe_set("@", "add readme").unwrap();
        let (uri, _) = run_diff_change(&jj, dir.path(), "@").expect("run_diff_change failed");
        let path = uri.strip_prefix("file://").unwrap();
        let content = std::fs::read_to_string(path).unwrap();
        assert!(
            content.starts_with("CHANGE_ID:"),
            "missing CHANGE_ID header:\n{content}"
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
    fn run_diff_change_with_empty_revision_defaults_to_at() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let (uri, _) = run_diff_change(&jj, dir.path(), "").expect("run_diff_change failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("CHANGE_ID:"),
            "expected CHANGE_ID header:\n{content}"
        );
    }

    #[test]
    fn run_diff_change_writes_file_to_badjuju_dir() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let (uri, _) = run_diff_change(&jj, dir.path(), "@").expect("run_diff_change failed");
        let path = uri.strip_prefix("file://").unwrap();
        assert!(
            path.contains(".jj/badjuju/diff-change-"),
            "unexpected path: {path}"
        );
        assert!(path.ends_with(".jujutsu"), "unexpected path: {path}");
    }

    #[test]
    fn diff_content_for_change_returns_change_id_header() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let change_id = jj.change_id_of("@").unwrap();
        let content = diff_content_for_change(&jj, &change_id).expect("should generate content");
        assert!(
            content.starts_with("CHANGE_ID:"),
            "expected CHANGE_ID header:\n{content}"
        );
        assert!(
            content.contains("DIFF:"),
            "expected DIFF section:\n{content}"
        );
    }

    #[test]
    fn diff_content_for_commit_returns_commit_id_header() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let commit_id = jj.commit_id_of("@").unwrap();
        let content = diff_content_for_commit(&jj, &commit_id).expect("should generate content");
        assert!(
            content.starts_with("COMMIT_ID:"),
            "expected COMMIT_ID header:\n{content}"
        );
        assert!(
            content.contains("DIFF:"),
            "expected DIFF section:\n{content}"
        );
    }

    #[test]
    fn file_blob_uri_virtual_returns_commit_scoped_uri() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "hello\n").unwrap();
        jj.describe_set("@", "add readme").unwrap();
        let commit_id = jj.commit_id_of("@").unwrap();
        let uri =
            file_blob_uri_virtual(&jj, "@", "readme.txt").expect("file_blob_uri_virtual failed");
        assert_eq!(
            uri,
            format!("badjuju-file:///commit/{commit_id}/readme.txt")
        );
    }

    #[test]
    fn file_blob_uri_virtual_invalid_revision_returns_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let result = file_blob_uri_virtual(&jj, "not-a-real-rev", "readme.txt");
        assert!(matches!(result, Err(CommandError::Jj(_))));
    }

    #[test]
    fn file_blob_with_path_writes_file_with_basename_extension() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("hello.rs"), "fn main() {}\n").unwrap();
        jj.describe_set("@", "add hello").unwrap();
        let uri =
            file_blob_with_path(&jj, dir.path(), "@", "hello.rs").expect("file_blob_with_path");
        assert!(uri.starts_with("file://"));
        let path = uri.strip_prefix("file://").unwrap();
        assert!(path.ends_with("/hello.rs"), "basename preserved: {path}");
        assert!(
            path.contains("/.jj/badjuju/blobs/"),
            "under blobs dir: {path}"
        );
        let content = std::fs::read_to_string(path).unwrap();
        assert_eq!(content, "fn main() {}\n");
    }

    #[test]
    fn file_blob_with_path_distinct_pairs_hash_to_distinct_dirs() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("a.txt"), "A\n").unwrap();
        std::fs::write(dir.path().join("b.txt"), "B\n").unwrap();
        jj.describe_set("@", "two files").unwrap();
        let uri_a = file_blob_with_path(&jj, dir.path(), "@", "a.txt").unwrap();
        let uri_b = file_blob_with_path(&jj, dir.path(), "@", "b.txt").unwrap();
        // Distinct paths under blobs/<hash>/ — same commit, different path → different hash.
        let parent_a = std::path::Path::new(uri_a.strip_prefix("file://").unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        let parent_b = std::path::Path::new(uri_b.strip_prefix("file://").unwrap())
            .parent()
            .unwrap()
            .to_path_buf();
        assert_ne!(
            parent_a, parent_b,
            "distinct (commit, path) → distinct dirs"
        );
    }

    #[test]
    fn file_blob_with_path_same_pair_is_idempotent() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("@", "add readme").unwrap();
        let uri1 = file_blob_with_path(&jj, dir.path(), "@", "readme.txt").unwrap();
        let uri2 = file_blob_with_path(&jj, dir.path(), "@", "readme.txt").unwrap();
        assert_eq!(uri1, uri2, "same (commit, path) → same URI");
    }

    #[test]
    fn file_content_at_commit_returns_content_at_revision() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("readme.txt"), "v1\n").unwrap();
        jj.describe_set("@", "add readme").unwrap();
        let commit_id = jj.commit_id_of("@").unwrap();
        // Mutate the working copy on a child change — file_content_at_commit
        // must still see the pinned commit's content.
        jj.new_change("").unwrap();
        std::fs::write(dir.path().join("readme.txt"), "v2\n").unwrap();
        let content = file_content_at_commit(&jj, &commit_id, "readme.txt")
            .expect("file_content_at_commit failed");
        assert_eq!(content, "v1\n");
    }

    #[test]
    fn file_content_at_commit_missing_path_returns_error() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let commit_id = jj.commit_id_of("@").unwrap();
        let result = file_content_at_commit(&jj, &commit_id, "does-not-exist.txt");
        assert!(matches!(result, Err(CommandError::Jj(_))));
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
        let (diff_uri, _) = run_diff_change(&jj, dir.path(), "@").unwrap();
        let refreshed = run_refresh(&jj, dir.path(), &diff_uri).expect("run_refresh failed");
        let content = std::fs::read_to_string(refreshed.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.starts_with("CHANGE_ID:"),
            "expected CHANGE_ID header:\n{content}"
        );
    }

    #[test]
    fn run_refresh_with_status_uri_regenerates_status() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let status_uri = run_status(&jj, dir.path()).unwrap();
        let refreshed = run_refresh(&jj, dir.path(), &status_uri).expect("run_refresh failed");
        assert!(refreshed.starts_with("file://"));
        let content = std::fs::read_to_string(refreshed.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.contains("@  :"));
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
        assert!(content.contains("@  :"));
    }

    #[test]
    fn run_new_writes_status_and_returns_uri() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        let uri = run_new(&jj, dir.path(), "").expect("run_new failed");
        assert!(uri.starts_with("file://"));
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(content.contains("@  :"));
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
        assert!(content.starts_with("@  :"));
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
        assert!(content.starts_with("@  :"));
        // readme.txt was squashed away from @, so it must not appear under
        // WORKING COPY CHANGES (it may legitimately appear under PARENT CHANGES).
        let at_changes_section = content
            .split("PARENT CHANGES")
            .next()
            .unwrap_or("")
            .split("STACK:")
            .next()
            .unwrap_or("");
        assert!(
            !at_changes_section.contains("readme.txt"),
            "expected readme.txt absent from @ working-copy section:\n{at_changes_section}"
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
        assert!(content.starts_with("@  :"));
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
            content.starts_with("@  :"),
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
            content.starts_with("@  :"),
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
            content.starts_with("@  :"),
            "expected @   header on successful no-op push, got:\n{content}"
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
            content.starts_with("@  :"),
            "expected @   header on push with force flag, got:\n{content}"
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
        assert!(content.starts_with("@  :"));
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
        assert!(content.starts_with("@  :"));
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
        assert!(content.starts_with("@  :"));
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
        assert!(content.starts_with("@  :"));
        // After abandoning @, the new working copy should be empty (no description carried over).
        let desc = jj.describe_get("@").unwrap();
        assert!(
            !desc.contains("a description"),
            "expected @ abandoned, but description survived: {desc}"
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
            content.starts_with("@  :"),
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
            content.starts_with("@  :"),
            "expected @   header, got:\n{content}"
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
            content.starts_with("@  :"),
            "expected @   header, got:\n{content}"
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
            content.starts_with("@  :"),
            "expected @   header, got:\n{content}"
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
            content.starts_with("@  :"),
            "expected @   header, got:\n{content}"
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
            "@  : (empty)",
            "@- : (empty)",
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

    #[test]
    fn status_folding_ranges_returns_empty_without_stack_section() {
        let content = "@  : (empty)\n@- : (empty)\n\nCOMMAND REFERENCE:\nkeys";
        let ranges = status_folding_ranges(content);
        assert!(ranges.is_empty(), "expected no ranges: {ranges:?}");
    }

    #[test]
    fn status_folding_ranges_returns_range_per_commit() {
        let content = concat!(
            "@  : (empty)\n@- : (empty)\n\n",
            "STACK: ancestors(@, 2)\n\n",
            "○  abcdefgh first commit\n",
            "│  M src/a.rs\n",
            "│  M src/b.rs\n",
            "@  qrstuvwx second commit\n",
            "   M src/c.rs\n",
            "\nCOMMAND REFERENCE:\nkeys",
        );
        let ranges = status_folding_ranges(content);
        assert_eq!(ranges.len(), 2, "expected 2 ranges, got: {ranges:?}");
        let first = &ranges[0];
        let second = &ranges[1];
        assert!(
            first.end_line > first.start_line,
            "first range should span multiple lines: {first:?}"
        );
        assert!(
            second.end_line > second.start_line,
            "second range should span multiple lines: {second:?}"
        );
        assert!(
            second.start_line > first.end_line,
            "ranges must not overlap: {first:?} vs {second:?}"
        );
    }

    #[test]
    fn write_status_includes_working_copy_changes_section() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("alpha.txt"), "a\n").unwrap();
        std::fs::write(dir.path().join("beta.txt"), "b\n").unwrap();
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.contains("WORKING COPY CHANGES ("),
            "expected WORKING COPY CHANGES section:\n{content}"
        );
        assert!(
            content.contains("alpha.txt"),
            "expected alpha.txt in WORKING COPY CHANGES:\n{content}"
        );
        assert!(
            content.contains("beta.txt"),
            "expected beta.txt in WORKING COPY CHANGES:\n{content}"
        );
    }

    #[test]
    fn write_status_omits_changes_sections_when_clean() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Fresh repo: @ has no changes, parent (root) has no changes.
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            !content.contains("WORKING COPY CHANGES"),
            "expected no WORKING COPY CHANGES when clean:\n{content}"
        );
        assert!(
            !content.contains("PARENT CHANGES"),
            "expected no PARENT CHANGES when clean:\n{content}"
        );
    }

    #[test]
    fn write_status_emits_parent_changes_section() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Write a file in parent, then create an empty child.
        std::fs::write(dir.path().join("baz.txt"), "baz\n").unwrap();
        jj.describe_set("@", "parent commit").unwrap();
        jj.new_change("").unwrap();
        // @ has no changes; @- has baz.txt.
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        assert!(
            content.contains("PARENT CHANGES ("),
            "expected PARENT CHANGES section:\n{content}"
        );
        assert!(
            content.contains("baz.txt"),
            "expected baz.txt in PARENT CHANGES:\n{content}"
        );
        assert!(
            !content.contains("WORKING COPY CHANGES"),
            "expected no WORKING COPY CHANGES for empty @:\n{content}"
        );
    }

    #[test]
    fn write_status_merge_emits_two_parent_changes_sections() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Create two parents each with a file.
        std::fs::write(dir.path().join("from-a.txt"), "a\n").unwrap();
        jj.describe_set("@", "branch-a").unwrap();
        let a_id = jj.change_ids("@").unwrap().first().cloned().unwrap();
        jj.new_change("").unwrap();
        std::fs::write(dir.path().join("from-b.txt"), "b\n").unwrap();
        jj.describe_set("@", "branch-b").unwrap();
        let b_id = jj.change_ids("@").unwrap().first().cloned().unwrap();
        // Merge.
        std::process::Command::new("jj")
            .args(["new", &a_id, &b_id])
            .current_dir(dir.path())
            .output()
            .expect("jj new (merge) failed");
        // @ is empty merge commit; it has two parents.
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        let parent_sections: Vec<_> = content
            .lines()
            .filter(|l| l.starts_with("PARENT CHANGES ("))
            .collect();
        assert_eq!(
            parent_sections.len(),
            2,
            "expected two PARENT CHANGES sections for a merge; got:\n{content}"
        );
        assert!(
            content.contains("from-a.txt"),
            "expected from-a.txt:\n{content}"
        );
        assert!(
            content.contains("from-b.txt"),
            "expected from-b.txt:\n{content}"
        );
    }

    #[test]
    fn write_status_changes_appear_before_stack() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("file.txt"), "hello\n").unwrap();
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        let changes_pos = content.find("WORKING COPY CHANGES (").unwrap();
        let stack_pos = content.find("STACK:").unwrap();
        assert!(
            changes_pos < stack_pos,
            "expected WORKING COPY CHANGES before STACK:\n{content}"
        );
    }

    #[test]
    fn status_folding_ranges_skips_commits_with_no_stat_lines() {
        let content = concat!(
            "@  : (empty)\n@- : (empty)\n\n",
            "STACK: ancestors(@, 2)\n\n",
            "@  abcdefgh commit with no files\n",
            "\nCOMMAND REFERENCE:\nkeys",
        );
        let ranges = status_folding_ranges(content);
        assert!(
            ranges.is_empty(),
            "commit with no stat lines should produce no range: {ranges:?}"
        );
    }

    #[test]
    fn hunks_for_single_hunk_single_file() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("foo.txt"), "old line\n").unwrap();
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        std::fs::write(dir.path().join("foo.txt"), "new line\n").unwrap();
        let hunks = hunks_for(&jj, "@", "foo.txt");
        assert!(hunks.contains("@@"), "expected @@ hunk header: {hunks}");
        assert!(
            hunks.contains("-old line"),
            "expected removed line: {hunks}"
        );
        assert!(hunks.contains("+new line"), "expected added line: {hunks}");
        assert!(
            !hunks.contains("diff --git"),
            "should not contain diff header: {hunks}"
        );
        assert!(
            !hunks.contains("--- ") && !hunks.contains("+++ "),
            "should not contain --- or +++ header lines: {hunks}"
        );
    }

    #[test]
    fn hunks_for_multi_hunk_file() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        // Write a file with many lines so two hunks appear far apart.
        let original: String = (1..=20).map(|i| format!("line {i}\n")).collect();
        std::fs::write(dir.path().join("foo.txt"), &original).unwrap();
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        let mut modified = original.clone();
        modified = modified.replacen("line 1\n", "CHANGED 1\n", 1);
        modified = modified.replacen("line 20\n", "CHANGED 20\n", 1);
        std::fs::write(dir.path().join("foo.txt"), &modified).unwrap();
        let hunks = hunks_for(&jj, "@", "foo.txt");
        let hunk_count = hunks.matches("@@").count();
        assert!(
            hunk_count >= 2,
            "expected multiple @@ blocks, got {hunk_count}: {hunks}"
        );
    }

    #[test]
    fn hunks_for_multi_file_hunks_land_under_correct_file() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("alpha.txt"), "aaa\n").unwrap();
        std::fs::write(dir.path().join("beta.txt"), "bbb\n").unwrap();
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        std::fs::write(dir.path().join("alpha.txt"), "AAA\n").unwrap();
        std::fs::write(dir.path().join("beta.txt"), "BBB\n").unwrap();
        let uri = run_status(&jj, dir.path()).expect("run_status failed");
        let content = std::fs::read_to_string(uri.strip_prefix("file://").unwrap()).unwrap();
        let alpha_pos = content.find("alpha.txt").unwrap();
        let beta_pos = content.find("beta.txt").unwrap();
        let alpha_hunk_pos = content[alpha_pos..]
            .find("@@")
            .map(|p| alpha_pos + p)
            .unwrap();
        let beta_hunk_pos = content[beta_pos..]
            .find("@@")
            .map(|p| beta_pos + p)
            .unwrap();
        assert!(
            alpha_pos < alpha_hunk_pos && alpha_hunk_pos < beta_pos,
            "alpha's @@ should appear after alpha.txt and before beta.txt: alpha={alpha_pos} hunk={alpha_hunk_pos} beta={beta_pos}"
        );
        assert!(
            beta_pos < beta_hunk_pos,
            "beta's @@ should appear after beta.txt: beta={beta_pos} hunk={beta_hunk_pos}"
        );
    }

    #[test]
    fn hunks_for_rename_only_produces_no_hunks() {
        let dir = tempdir().unwrap();
        let jj = init_repo(dir.path());
        std::fs::write(dir.path().join("old.txt"), "content\n").unwrap();
        jj.describe_set("@", "parent").unwrap();
        jj.new_change("").unwrap();
        // Perform a rename by removing old and adding new with same content.
        std::fs::rename(dir.path().join("old.txt"), dir.path().join("new.txt")).unwrap();
        // jj should detect this as a rename; diff for the destination has no content change.
        let hunks = hunks_for(&jj, "@", "new.txt");
        // A pure rename may produce no @@ lines since content is identical.
        // We just assert no crash and that if there are no hunks, the string is empty or contains no diff content.
        assert!(
            !hunks.contains("-content") && !hunks.contains("+content"),
            "rename-only should not show content additions/removals: {hunks}"
        );
    }

    // Helper: build a synthetic status buffer with one CHANGES section and one STACK commit.
    fn make_status_with_changes(section_header: &str, file: &str, hunks: &str) -> String {
        let stack = concat!(
            "STACK: ancestors(@, 2)\n\n",
            "○  abcdefgh some commit\n",
            "   M src/a.rs\n",
            "\nCOMMAND REFERENCE:\nkeys"
        );
        if hunks.is_empty() {
            format!("@  : (empty)\n@- : (empty)\n\n{section_header}\n{file}\n\n{stack}")
        } else {
            format!("@  : (empty)\n@- : (empty)\n\n{section_header}\n{file}\n{hunks}\n\n{stack}")
        }
    }

    #[test]
    fn status_folding_ranges_section_fold_spans_heading_to_last_hunk_line() {
        let content = make_status_with_changes(
            "WORKING COPY CHANGES (abc12345):",
            "foo.txt",
            "@@ -1,1 +1,1 @@\n-old\n+new",
        );
        let ranges = status_folding_ranges(&content);
        let lines: Vec<&str> = content.lines().collect();
        let section_line = lines
            .iter()
            .position(|l| l.starts_with("WORKING COPY CHANGES"))
            .unwrap() as u32;
        let section_fold = ranges
            .iter()
            .find(|r| r.start_line == section_line)
            .expect("expected section fold");
        let last_plus_line = lines
            .iter()
            .rposition(|l| l.starts_with('+') || l.starts_with('-'))
            .unwrap() as u32;
        assert_eq!(
            section_fold.end_line, last_plus_line,
            "section fold should end at last hunk content line"
        );
    }

    #[test]
    fn status_folding_ranges_file_fold_contained_in_section_fold() {
        let content = make_status_with_changes(
            "WORKING COPY CHANGES (abc12345):",
            "foo.txt",
            "@@ -1,1 +1,1 @@\n-old\n+new",
        );
        let ranges = status_folding_ranges(&content);
        let lines: Vec<&str> = content.lines().collect();
        let section_line = lines
            .iter()
            .position(|l| l.starts_with("WORKING COPY CHANGES"))
            .unwrap() as u32;
        let file_line = lines.iter().position(|l| *l == "foo.txt").unwrap() as u32;
        let section_fold = ranges
            .iter()
            .find(|r| r.start_line == section_line)
            .expect("section fold");
        let file_fold = ranges
            .iter()
            .find(|r| r.start_line == file_line)
            .expect("file fold");
        assert!(
            file_fold.start_line >= section_fold.start_line
                && file_fold.end_line <= section_fold.end_line,
            "file fold must be contained in section fold: file={file_fold:?} section={section_fold:?}"
        );
    }

    #[test]
    fn status_folding_ranges_hunk_fold_contained_in_file_fold() {
        let content = make_status_with_changes(
            "WORKING COPY CHANGES (abc12345):",
            "foo.txt",
            "@@ -1,1 +1,1 @@\n-old\n+new",
        );
        let ranges = status_folding_ranges(&content);
        let lines: Vec<&str> = content.lines().collect();
        let file_line = lines.iter().position(|l| *l == "foo.txt").unwrap() as u32;
        let hunk_line = lines.iter().position(|l| l.starts_with("@@")).unwrap() as u32;
        let file_fold = ranges
            .iter()
            .find(|r| r.start_line == file_line)
            .expect("file fold");
        let hunk_fold = ranges
            .iter()
            .find(|r| r.start_line == hunk_line)
            .expect("hunk fold");
        assert!(
            hunk_fold.start_line >= file_fold.start_line
                && hunk_fold.end_line <= file_fold.end_line,
            "hunk fold must be inside file fold: hunk={hunk_fold:?} file={file_fold:?}"
        );
    }

    #[test]
    fn status_folding_ranges_multi_file_multi_hunk_emits_expected_count() {
        // 2 files, 1 hunk each → 1 section + 2 file + 2 hunk + 1 stack = 6 ranges
        let content = concat!(
            "@  : (empty)\n@- : (empty)\n\n",
            "WORKING COPY CHANGES (abc12345):\n",
            "alpha.txt\n",
            "@@ -1,1 +1,1 @@\n",
            "-aaa\n",
            "+AAA\n",
            "beta.txt\n",
            "@@ -1,1 +1,1 @@\n",
            "-bbb\n",
            "+BBB\n",
            "\n",
            "STACK: ancestors(@, 2)\n\n",
            "○  abcdefgh commit\n",
            "   M src/a.rs\n",
            "\nCOMMAND REFERENCE:\nkeys",
        );
        let ranges = status_folding_ranges(content);
        assert_eq!(
            ranges.len(),
            6,
            "expected 6 ranges (1 section + 2 file + 2 hunk + 1 stack): {ranges:?}"
        );
    }

    #[test]
    fn status_folding_ranges_stack_folds_still_present_with_changes() {
        let content = concat!(
            "@  : (empty)\n@- : (empty)\n\n",
            "WORKING COPY CHANGES (abc12345):\n",
            "foo.txt\n",
            "@@ -1,1 +1,1 @@\n",
            "-old\n",
            "+new\n",
            "\n",
            "STACK: ancestors(@, 2)\n\n",
            "○  abcdefgh first commit\n",
            "│  M src/a.rs\n",
            "@  qrstuvwx second commit\n",
            "   M src/b.rs\n",
            "\nCOMMAND REFERENCE:\nkeys",
        );
        let ranges = status_folding_ranges(content);
        // 1 section + 1 file + 1 hunk + 2 stack = 5
        assert_eq!(
            ranges.len(),
            5,
            "expected 5 ranges with stack folds: {ranges:?}"
        );
        // The two stack commit folds should be the last two ranges emitted.
        let stack_folds: Vec<_> = ranges
            .iter()
            .filter(|r| {
                let lines: Vec<&str> = content.lines().collect();
                let line = lines.get(r.start_line as usize).unwrap_or(&"");
                crate::cursor::match_commit_header(line).is_some()
            })
            .collect();
        assert_eq!(stack_folds.len(), 2, "expected 2 stack folds: {ranges:?}");
    }

    #[test]
    fn status_folding_ranges_all_have_region_kind() {
        let content = concat!(
            "@  : (empty)\n@- : (empty)\n\n",
            "WORKING COPY CHANGES (abc12345):\n",
            "foo.txt\n",
            "@@ -1,1 +1,1 @@\n",
            "-old\n",
            "+new\n",
            "\n",
            "STACK: ancestors(@, 2)\n\n",
            "○  abcdefgh commit\n",
            "   M src/a.rs\n",
            "\nCOMMAND REFERENCE:\nkeys",
        );
        let ranges = status_folding_ranges(content);
        for r in &ranges {
            assert_eq!(
                r.kind,
                Some(tower_lsp::lsp_types::FoldingRangeKind::Region),
                "all ranges must have kind=Region: {r:?}"
            );
        }
    }

    // ---- Hunk-edit buffer (#13) ----

    #[test]
    fn parse_hunk_edit_buffer_strips_jj_lines_and_extracts_body() {
        let text = "\
JJ: Edit the +/- lines below, then save to apply.
JJ: action: squash
JJ: from: abc
JJ: into: def
JJ: file: src/a.rs
JJ: original-header: @@ -1,2 +1,3 @@

src/a.rs
@@ -1,2 +1,3 @@
 keep
-old
+new1
+new2

COMMAND REFERENCE:
(save) apply
";
        let (header, body) = parse_hunk_edit_buffer(text).expect("buffer should parse");
        assert_eq!(header, "@@ -1,2 +1,3 @@");
        assert_eq!(body, " keep\n-old\n+new1\n+new2");
    }

    #[test]
    fn parse_hunk_edit_buffer_returns_none_on_empty_body() {
        let text = "\
JJ: action: squash
JJ: from: abc
JJ: into: def

src/a.rs
@@ -1,2 +1,3 @@

COMMAND REFERENCE:
";
        assert!(parse_hunk_edit_buffer(text).is_none());
    }

    #[test]
    fn recompute_hunk_header_preserves_starts_and_recounts_lengths() {
        let original = "@@ -10,2 +10,3 @@";
        let body = " ctx\n-removed\n+added1\n+added2";
        let header = recompute_hunk_header(original, body).unwrap();
        // 1 context + 1 removal = old_len 2; 1 context + 2 additions = new_len 3.
        assert_eq!(header, "@@ -10,2 +10,3 @@");
    }

    #[test]
    fn recompute_hunk_header_pure_addition() {
        let original = "@@ -5,0 +5,2 @@";
        let body = "+a\n+b";
        let header = recompute_hunk_header(original, body).unwrap();
        assert_eq!(header, "@@ -5,0 +5,2 @@");
    }

    #[test]
    fn recompute_hunk_header_pure_deletion() {
        let original = "@@ -7,2 +7,0 @@";
        let body = "-a\n-b";
        let header = recompute_hunk_header(original, body).unwrap();
        assert_eq!(header, "@@ -7,2 +7,0 @@");
    }

    #[test]
    fn recompute_hunk_header_rejects_unknown_prefix() {
        let original = "@@ -1,1 +1,1 @@";
        let body = "*nope";
        let err = recompute_hunk_header(original, body).unwrap_err();
        assert!(matches!(err, CommandError::Io(_)));
    }

    #[test]
    fn render_hunk_edit_buffer_contains_metadata_and_body() {
        let buf = render_hunk_edit_buffer(
            "squash",
            "abcd",
            "efgh",
            "src/a.rs",
            "@@ -1,1 +1,1 @@",
            "-old\n+new\n",
            "COMMAND REFERENCE:\n(save) apply",
        );
        assert!(buf.contains("JJ: action: squash"));
        assert!(buf.contains("JJ: from: abcd"));
        assert!(buf.contains("JJ: into: efgh"));
        assert!(buf.contains("JJ: file: src/a.rs"));
        assert!(buf.contains("JJ: original-header: @@ -1,1 +1,1 @@"));
        assert!(buf.contains("\nsrc/a.rs\n@@ -1,1 +1,1 @@\n-old\n+new"));
        assert!(buf.contains("COMMAND REFERENCE:"));
    }

    #[test]
    fn find_change_id_line_matches_short_prefix_in_stack_row() {
        // Status buffer STACK section format: `<graph>  <short-id> <commit-id> ...`.
        // The short-id rendered by jj is a prefix of the full change-id we look up.
        let content = "\
@  : working copy
@- : parent

STACK: ancestors(reachable(@, mutable()), 2)

@  abcd1234 def67890 1min stephen@example.com
│  working copy description
○  ffff5678 cccc9999 2min stephen@example.com
│  parent description

COMMAND REFERENCE:
";
        let full_change_id = "abcd1234aaaaaaaaaaaaaaaaaaaaaaaa";
        let line = find_change_id_line(content, full_change_id).expect("expected match");
        assert_eq!(
            content.lines().nth(line as usize).unwrap(),
            "@  abcd1234 def67890 1min stephen@example.com"
        );

        // The other commit on a different row should also be findable.
        let other = "ffff5678bbbbbbbbbbbbbbbbbbbbbbbb";
        let other_line = find_change_id_line(content, other).expect("expected match");
        assert_eq!(
            content.lines().nth(other_line as usize).unwrap(),
            "○  ffff5678 cccc9999 2min stephen@example.com"
        );
    }

    #[test]
    fn find_change_id_line_returns_none_when_not_in_content() {
        let content = "\
STACK:

@  abcd1234 def67890 ...
";
        let absent = "9999999999999999";
        assert!(find_change_id_line(content, absent).is_none());
    }
}
