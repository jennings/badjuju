use serde::Serialize;

/// A single entry in a keymap profile table.
#[derive(Debug, Clone, Serialize)]
pub struct KeymapEntry {
    pub key: &'static str,
    /// Stable LSP command identifier (e.g. "badjuju.new").
    pub action: &'static str,
    pub description: &'static str,
    /// Buffer types this entry applies to.
    pub windows: &'static [&'static str],
}

/// Named keymap profiles selectable via `initializationOptions.keymapProfile`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum KeymapProfile {
    #[default]
    Magit,
    Vim,
    None,
}

impl KeymapProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Magit => "magit",
            Self::Vim => "vim",
            Self::None => "none",
        }
    }
}

impl std::str::FromStr for KeymapProfile {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "magit" => Ok(Self::Magit),
            "vim" => Ok(Self::Vim),
            "none" => Ok(Self::None),
            _ => Err(()),
        }
    }
}

static MAGIT_ENTRIES: &[KeymapEntry] = &[
    // Status-only
    KeymapEntry {
        key: "n",
        action: "badjuju.new",
        description: "new change",
        windows: &["status"],
    },
    KeymapEntry {
        key: "L",
        action: "badjuju.log",
        description: "open log",
        windows: &["status"],
    },
    KeymapEntry {
        key: "l f",
        action: "badjuju.log.file",
        description: "log for file at cursor",
        windows: &["status"],
    },
    KeymapEntry {
        key: "s",
        action: "badjuju.squash.commit",
        description: "select squash source or destination",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "S",
        action: "badjuju.squash.cancel",
        description: "cancel pending squash / squash file at cursor",
        windows: &["status"],
    },
    KeymapEntry {
        key: "S",
        action: "badjuju.squash.cancel",
        description: "cancel pending squash",
        windows: &["log"],
    },
    KeymapEntry {
        key: "u",
        action: "badjuju.unsquash",
        description: "unsquash file at cursor from parent into child",
        windows: &["status"],
    },
    KeymapEntry {
        key: "f",
        action: "badjuju.fetch",
        description: "git fetch",
        windows: &["status"],
    },
    KeymapEntry {
        key: "p",
        action: "badjuju.push",
        description: "git push",
        windows: &["status"],
    },
    KeymapEntry {
        key: "P",
        action: "badjuju.push",
        description: "git push (force)",
        windows: &["status"],
    },
    KeymapEntry {
        key: "U",
        action: "badjuju.undo",
        description: "jj undo",
        windows: &["status", "log"],
    },
    // Bookmark chord
    KeymapEntry {
        key: "b c",
        action: "badjuju.bookmark",
        description: "create bookmark",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "b m",
        action: "badjuju.bookmark",
        description: "move bookmark",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "b d",
        action: "badjuju.bookmark",
        description: "delete bookmark",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "b t",
        action: "badjuju.bookmark",
        description: "track bookmark",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "b f",
        action: "badjuju.bookmark",
        description: "forget bookmark",
        windows: &["status", "log"],
    },
    // Commit chord
    KeymapEntry {
        key: "c n",
        action: "badjuju.new",
        description: "new commit",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "c w",
        action: "badjuju.describe",
        description: "describe commit (reword)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "r",
        action: "badjuju.rebase",
        description: "rebase to destination",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "e",
        action: "badjuju.edit",
        description: "edit commit at cursor (move @)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "d",
        action: "badjuju.diff",
        description: "diff change at cursor (updates on amend)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "D",
        action: "badjuju.diff.commit",
        description: "diff commit at cursor (pinned, immutable)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "=",
        action: "badjuju.diff",
        description: "diff change at cursor (alias for d)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "a",
        action: "badjuju.abandon",
        description: "abandon commit at cursor",
        windows: &["status", "log"],
    },
    // RET — context-dispatched: apply revset shortcut on JJ: shortcut lines
    // (log buffer only), otherwise invoke textDocument/definition at point.
    KeymapEntry {
        key: "RET",
        action: "(editor)",
        description: "apply revset shortcut / goto definition",
        windows: &["status", "log", "diff"],
    },
    // All main windows
    KeymapEntry {
        key: "R",
        action: "badjuju.refresh",
        description: "refresh",
        windows: &["status", "log", "diff"],
    },
    // Fold toggle
    KeymapEntry {
        key: "Tab",
        action: "(editor)",
        description: "toggle fold at cursor",
        windows: &["status", "squash"],
    },
    // Close (all main windows)
    KeymapEntry {
        key: "q",
        action: "",
        description: "close",
        windows: &["status", "log", "diff"],
    },
    // Help (all windows)
    KeymapEntry {
        key: "?",
        action: "badjuju.help",
        description: "show help",
        windows: &["status", "log", "diff", "describe"],
    },
    // Describe-only
    KeymapEntry {
        key: "Ctrl-c Ctrl-c",
        action: "badjuju.describe.finalize",
        description: "finalize commit (save and close)",
        windows: &["describe"],
    },
    KeymapEntry {
        key: "Ctrl-c Ctrl-k",
        action: "badjuju.describe.abort",
        description: "abort (close without saving)",
        windows: &["describe"],
    },
    // Squash window
    KeymapEntry {
        key: "s",
        action: "badjuju.squash.toggle",
        description: "toggle hunk or file (move between selected/remaining)",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "e",
        action: "badjuju.squash.edit_hunk",
        description: "edit hunk before squashing",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "a",
        action: "badjuju.squash.select_all",
        description: "select all changes",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "A",
        action: "badjuju.squash.select_none",
        description: "deselect all changes",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "u",
        action: "badjuju.undo",
        description: "jj undo",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "q",
        action: "",
        description: "close",
        windows: &["squash"],
    },
    // Hunk-edit buffer — save to apply, close to discard.
    KeymapEntry {
        key: "(save)",
        action: "(editor)",
        description: "apply edited hunk and close",
        windows: &["hunk-edit"],
    },
    KeymapEntry {
        key: "q",
        action: "",
        description: "close (discard pending edit)",
        windows: &["hunk-edit"],
    },
];

/// Fugitive-style two-letter verb profile.
static VIM_ENTRIES: &[KeymapEntry] = &[
    // Status-only
    KeymapEntry {
        key: "nn",
        action: "badjuju.new",
        description: "new change",
        windows: &["status"],
    },
    KeymapEntry {
        key: "ll",
        action: "badjuju.log",
        description: "open log",
        windows: &["status"],
    },
    KeymapEntry {
        key: "l f",
        action: "badjuju.log.file",
        description: "log for file at cursor",
        windows: &["status"],
    },
    KeymapEntry {
        key: "ss",
        action: "badjuju.squash.commit",
        description: "select squash source or destination",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "SS",
        action: "badjuju.squash.cancel",
        description: "cancel pending squash / squash file at cursor",
        windows: &["status"],
    },
    KeymapEntry {
        key: "SS",
        action: "badjuju.squash.cancel",
        description: "cancel pending squash",
        windows: &["log"],
    },
    KeymapEntry {
        key: "uu",
        action: "badjuju.unsquash",
        description: "unsquash file at cursor from parent into child",
        windows: &["status"],
    },
    KeymapEntry {
        key: "ff",
        action: "badjuju.fetch",
        description: "git fetch",
        windows: &["status"],
    },
    KeymapEntry {
        key: "pp",
        action: "badjuju.push",
        description: "git push",
        windows: &["status"],
    },
    KeymapEntry {
        key: "PP",
        action: "badjuju.push",
        description: "git push (force)",
        windows: &["status"],
    },
    KeymapEntry {
        key: "UU",
        action: "badjuju.undo",
        description: "jj undo",
        windows: &["status", "log"],
    },
    // Status + log
    // Bookmark chord
    KeymapEntry {
        key: "bb c",
        action: "badjuju.bookmark",
        description: "create bookmark",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "bb m",
        action: "badjuju.bookmark",
        description: "move bookmark",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "bb d",
        action: "badjuju.bookmark",
        description: "delete bookmark",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "bb t",
        action: "badjuju.bookmark",
        description: "track bookmark",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "bb f",
        action: "badjuju.bookmark",
        description: "forget bookmark",
        windows: &["status", "log"],
    },
    // Commit chord
    KeymapEntry {
        key: "c n",
        action: "badjuju.new",
        description: "new commit",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "c w",
        action: "badjuju.describe",
        description: "describe commit (reword)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "rr",
        action: "badjuju.rebase",
        description: "rebase to destination",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "ee",
        action: "badjuju.edit",
        description: "edit commit at cursor (move @)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "dd",
        action: "badjuju.describe",
        description: "describe commit at cursor",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "d",
        action: "badjuju.diff",
        description: "diff change at cursor (updates on amend)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "D",
        action: "badjuju.diff.commit",
        description: "diff commit at cursor (pinned, immutable)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "=",
        action: "badjuju.diff",
        description: "diff change at cursor (alias for d)",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "aa",
        action: "badjuju.abandon",
        description: "abandon commit at cursor",
        windows: &["status", "log"],
    },
    // All main windows
    KeymapEntry {
        key: "R",
        action: "badjuju.refresh",
        description: "refresh",
        windows: &["status", "log", "diff"],
    },
    // Fold toggle
    KeymapEntry {
        key: "Tab",
        action: "(editor)",
        description: "toggle fold at cursor",
        windows: &["status", "squash"],
    },
    // Close
    KeymapEntry {
        key: "q",
        action: "",
        description: "close",
        windows: &["status", "log", "diff"],
    },
    // Help
    KeymapEntry {
        key: "?",
        action: "badjuju.help",
        description: "show help",
        windows: &["status", "log", "diff", "describe"],
    },
    // Describe-only
    KeymapEntry {
        key: "Ctrl-c Ctrl-c",
        action: "badjuju.describe.finalize",
        description: "finalize commit (save and close)",
        windows: &["describe"],
    },
    KeymapEntry {
        key: "Ctrl-c Ctrl-k",
        action: "badjuju.describe.abort",
        description: "abort (close without saving)",
        windows: &["describe"],
    },
    // Squash window
    KeymapEntry {
        key: "s",
        action: "badjuju.squash.toggle",
        description: "toggle hunk or file (move between selected/remaining)",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "e",
        action: "badjuju.squash.edit_hunk",
        description: "edit hunk before squashing",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "a",
        action: "badjuju.squash.select_all",
        description: "select all changes",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "A",
        action: "badjuju.squash.select_none",
        description: "deselect all changes",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "u",
        action: "badjuju.undo",
        description: "jj undo",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "q",
        action: "",
        description: "close",
        windows: &["squash"],
    },
    KeymapEntry {
        key: "(save)",
        action: "(editor)",
        description: "apply edited hunk and close",
        windows: &["hunk-edit"],
    },
    KeymapEntry {
        key: "q",
        action: "",
        description: "close (discard pending edit)",
        windows: &["hunk-edit"],
    },
];

/// Return the keymap entries for a given profile and window type.
pub fn entries_for_window(profile: &KeymapProfile, window: &str) -> Vec<&'static KeymapEntry> {
    let table = match profile {
        KeymapProfile::Magit => MAGIT_ENTRIES,
        KeymapProfile::Vim => VIM_ENTRIES,
        KeymapProfile::None => return vec![],
    };
    table
        .iter()
        .filter(|e| e.windows.contains(&window))
        .collect()
}

/// Render the COMMAND REFERENCE block for the given profile and window type.
pub fn render_command_reference(profile: &KeymapProfile, window: &str) -> String {
    let entries = entries_for_window(profile, window);
    let mut lines = vec!["COMMAND REFERENCE:".to_string()];

    if *profile == KeymapProfile::None {
        lines.push(
            "No default bindings active. Configure your own hotkeys via editor keybinding settings."
                .to_string(),
        );
        return lines.join("\n");
    }

    if window == "log" {
        lines.push("Edit REVSET above and save to re-run the query.".to_string());
        lines.push("Place the cursor on a shortcut line and press Enter to apply it.".to_string());
    }

    let max_key = entries.iter().map(|e| e.key.len()).max().unwrap_or(0);
    for e in entries {
        let pad = " ".repeat(max_key - e.key.len() + 3);
        lines.push(format!("{}{}{}", e.key, pad, e.description));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_status_contains_all_expected_keys() {
        let text = render_command_reference(&KeymapProfile::Magit, "status");
        assert!(text.starts_with("COMMAND REFERENCE:"));
        for key in [
            "n", "L", "c", "b", "r", "e", "d", "D", "s", "S", "U", "a", "f", "p", "P", "u", "R",
            "q", "?", "Tab", "RET",
        ] {
            assert!(
                text.lines().any(|l| l.starts_with(key)),
                "missing key `{key}` in:\n{text}"
            );
        }
    }

    #[test]
    fn render_log_contains_prose_intro() {
        let text = render_command_reference(&KeymapProfile::Magit, "log");
        assert!(
            text.contains("Edit REVSET above"),
            "missing log intro in:\n{text}"
        );
        assert!(
            text.contains("shortcut line"),
            "missing shortcut hint in:\n{text}"
        );
        for key in [
            "c", "b", "r", "e", "d", "D", "s", "S", "a", "R", "q", "?", "RET",
        ] {
            assert!(
                text.lines().any(|l| l.starts_with(key)),
                "missing key `{key}` in log:\n{text}"
            );
        }
    }

    #[test]
    fn render_diff_has_refresh_close_help() {
        let text = render_command_reference(&KeymapProfile::Magit, "diff");
        for key in ["R", "q", "?", "RET"] {
            assert!(
                text.lines().any(|l| l.starts_with(key)),
                "missing key `{key}` in diff:\n{text}"
            );
        }
    }

    #[test]
    fn render_squash_has_toggle_select_close() {
        let text = render_command_reference(&KeymapProfile::Magit, "squash");
        for key in ["s", "a", "A", "u", "q", "Tab"] {
            assert!(
                text.lines().any(|l| l.starts_with(key)),
                "missing key `{key}` in squash:\n{text}"
            );
        }
    }

    #[test]
    fn render_squash_has_undo_key_in_both_profiles() {
        for profile in [KeymapProfile::Magit, KeymapProfile::Vim] {
            let text = render_command_reference(&profile, "squash");
            assert!(
                text.lines()
                    .any(|l| l.starts_with('u') && l.contains("undo")),
                "missing `u` undo binding in {profile:?} squash:\n{text}"
            );
        }
    }

    #[test]
    fn render_squash_has_tab_fold_toggle() {
        for profile in [KeymapProfile::Magit, KeymapProfile::Vim] {
            let text = render_command_reference(&profile, "squash");
            assert!(
                text.lines().any(|l| l.starts_with("Tab")),
                "missing Tab fold toggle in {profile:?} squash:\n{text}"
            );
            assert!(
                text.contains("toggle fold"),
                "missing fold description in {profile:?} squash:\n{text}"
            );
        }
    }

    #[test]
    fn render_squash_has_edit_key() {
        for profile in [KeymapProfile::Magit, KeymapProfile::Vim] {
            let text = render_command_reference(&profile, "squash");
            assert!(
                text.lines().any(|l| l.starts_with('e')),
                "missing `e` key for edit_hunk in {profile:?} squash:\n{text}"
            );
            assert!(
                text.contains("edit hunk"),
                "missing edit-hunk description in {profile:?} squash:\n{text}"
            );
        }
    }

    #[test]
    fn render_hunk_edit_has_command_reference_block() {
        for profile in [KeymapProfile::Magit, KeymapProfile::Vim] {
            let text = render_command_reference(&profile, "hunk-edit");
            assert!(
                text.starts_with("COMMAND REFERENCE:"),
                "missing header in {profile:?} hunk-edit:\n{text}"
            );
            assert!(
                text.contains("apply edited hunk"),
                "missing save action in {profile:?} hunk-edit:\n{text}"
            );
            assert!(
                text.contains("close"),
                "missing close action in {profile:?} hunk-edit:\n{text}"
            );
        }
    }

    #[test]
    fn render_describe_has_finalize_abort_help() {
        let text = render_command_reference(&KeymapProfile::Magit, "describe");
        assert!(text.starts_with("COMMAND REFERENCE:"));
        assert!(text.contains("Ctrl-c Ctrl-c"), "missing finalize key");
        assert!(text.contains("Ctrl-c Ctrl-k"), "missing abort key");
        assert!(text.lines().any(|l| l.starts_with('?')), "missing help key");
    }

    #[test]
    fn none_profile_returns_empty_entries() {
        assert!(entries_for_window(&KeymapProfile::None, "status").is_empty());
    }

    #[test]
    fn none_profile_renders_no_bindings_note() {
        let text = render_command_reference(&KeymapProfile::None, "status");
        assert!(text.starts_with("COMMAND REFERENCE:"), "missing header");
        assert!(
            text.contains("No default bindings active"),
            "missing none-profile note in:\n{text}"
        );
    }

    #[test]
    fn profile_from_str_round_trips() {
        assert_eq!("magit".parse::<KeymapProfile>(), Ok(KeymapProfile::Magit));
        assert_eq!("vim".parse::<KeymapProfile>(), Ok(KeymapProfile::Vim));
        assert_eq!("none".parse::<KeymapProfile>(), Ok(KeymapProfile::None));
        assert!("dvorak".parse::<KeymapProfile>().is_err());
        assert!("".parse::<KeymapProfile>().is_err());
    }

    #[test]
    fn vim_profile_status_contains_two_letter_verbs() {
        let text = render_command_reference(&KeymapProfile::Vim, "status");
        assert!(text.starts_with("COMMAND REFERENCE:"));
        for key in [
            "nn", "ll", "ff", "pp", "PP", "uu", "ss", "UU", "bb", "rr", "ee", "dd", "D", "aa", "R",
            "q", "?", "Tab",
        ] {
            assert!(
                text.lines().any(|l| l.starts_with(key)),
                "missing key `{key}` in vim status:\n{text}"
            );
        }
    }

    #[test]
    fn vim_profile_log_contains_two_letter_verbs() {
        let text = render_command_reference(&KeymapProfile::Vim, "log");
        for key in ["bb", "rr", "ee", "dd", "D", "aa", "R", "q", "?"] {
            assert!(
                text.lines().any(|l| l.starts_with(key)),
                "missing key `{key}` in vim log:\n{text}"
            );
        }
    }

    #[test]
    fn vim_entries_for_status_nonempty() {
        let entries = entries_for_window(&KeymapProfile::Vim, "status");
        assert!(
            !entries.is_empty(),
            "vim profile should have status entries"
        );
    }
}
