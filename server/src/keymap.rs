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
    None,
}

impl KeymapProfile {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Magit => "magit",
            Self::None => "none",
        }
    }
}

impl std::str::FromStr for KeymapProfile {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "magit" => Ok(Self::Magit),
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
        key: "l",
        action: "badjuju.log",
        description: "open log",
        windows: &["status"],
    },
    KeymapEntry {
        key: "s",
        action: "badjuju.squash",
        description: "squash file at cursor into parent",
        windows: &["status"],
    },
    KeymapEntry {
        key: "U",
        action: "badjuju.unsquash",
        description: "unsquash file at cursor from parent into child",
        windows: &["status"],
    },
    KeymapEntry {
        key: "u",
        action: "badjuju.undo",
        description: "jj undo",
        windows: &["status"],
    },
    KeymapEntry {
        key: "=",
        action: "badjuju.toggleStat",
        description: "toggle --stat on the stack log",
        windows: &["status"],
    },
    // Status + log
    KeymapEntry {
        key: "d",
        action: "badjuju.describe",
        description: "describe commit at cursor",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "D",
        action: "badjuju.diff",
        description: "diff commit at cursor",
        windows: &["status", "log"],
    },
    KeymapEntry {
        key: "a",
        action: "badjuju.abandon",
        description: "abandon commit at cursor",
        windows: &["status", "log"],
    },
    // All windows
    KeymapEntry {
        key: "g",
        action: "badjuju.refresh",
        description: "refresh (or r)",
        windows: &["status", "log", "diff"],
    },
    // Status + diff close
    KeymapEntry {
        key: "q",
        action: "",
        description: "close",
        windows: &["status", "diff"],
    },
];

/// Return the keymap entries for a given profile and window type.
pub fn entries_for_window(
    profile: &KeymapProfile,
    window: &str,
) -> Vec<&'static KeymapEntry> {
    let table = match profile {
        KeymapProfile::Magit => MAGIT_ENTRIES,
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

    if window == "log" {
        lines.push("Edit REVSET above and save to re-run the query.".to_string());
        lines.push(
            "Place the cursor on a shortcut line and press Enter to apply it.".to_string(),
        );
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
        for key in ["n", "l", "d", "D", "s", "U", "a", "u", "=", "g", "q"] {
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
        assert!(text.contains("shortcut line"), "missing shortcut hint in:\n{text}");
        assert!(text.lines().any(|l| l.starts_with("d")));
        assert!(text.lines().any(|l| l.starts_with("a")));
    }

    #[test]
    fn render_diff_has_g_and_q() {
        let text = render_command_reference(&KeymapProfile::Magit, "diff");
        assert!(text.lines().any(|l| l.starts_with("g")));
        assert!(text.lines().any(|l| l.starts_with("q")));
    }

    #[test]
    fn none_profile_returns_empty_entries() {
        assert!(entries_for_window(&KeymapProfile::None, "status").is_empty());
    }

    #[test]
    fn none_profile_renders_header_only() {
        let text = render_command_reference(&KeymapProfile::None, "status");
        assert_eq!(text, "COMMAND REFERENCE:");
    }

    #[test]
    fn profile_from_str_round_trips() {
        assert_eq!("magit".parse::<KeymapProfile>(), Ok(KeymapProfile::Magit));
        assert_eq!("none".parse::<KeymapProfile>(), Ok(KeymapProfile::None));
        assert!("vim".parse::<KeymapProfile>().is_err());
        assert!("".parse::<KeymapProfile>().is_err());
    }
}
