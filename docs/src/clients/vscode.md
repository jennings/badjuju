# VS Code

The VS Code extension lives in `clients/vscode/` and is the most
feature-complete client today.

## Requirements

- VS Code (any recent version)
- `badjuju` server binary on your `PATH` (see [Getting
  Started](../getting-started.md#1-install-the-server))

## Installation

### From the marketplace

(Coming soon.) For now, build and install a local VSIX.

### Build a local VSIX

```sh
# From the repo root:
redo clients/vscode/install
```

This builds the VSIX for your platform and installs it via
`code --install-extension --force`. The `code` CLI must be on your
`PATH` — in VS Code, run **Shell Command: Install 'code' command in
PATH** from the Command Palette.

For other platforms or a packaged set of VSIXs:

```sh
# Single non-host VSIX
TARGET=x86_64-unknown-linux-gnu redo clients/vscode/all

# All platforms at once (requires zig + cargo-zigbuild)
redo clients/vscode/pack
```

## Commands

Open the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`) and type
`jj` to filter.

| Command ID | Palette name | Description |
| ---------- | ------------ | ----------- |
| `badjuju.status.open` | jj: Status | Open status / change stack |
| `badjuju.log.open` | jj: Open log | Open the revision log |
| `badjuju.describe.open` | jj: Describe working copy | Edit the current commit message |
| `badjuju.new.open` | jj: New commit | Create a new empty change |
| `badjuju.next.open` | jj: Move forward (jj next) | Move @ to next child |
| `badjuju.next.edit` | jj: Edit next in place | Edit next change in place |
| `badjuju.prev.open` | jj: Move back (jj prev) | Move @ to previous parent |
| `badjuju.prev.edit` | jj: Edit previous in place | Edit previous change in place |
| `badjuju.refresh.open` | jj: Refresh | Re-run the current buffer's command |
| `badjuju.undo.open` | jj: Undo last operation | Undo last jj op |
| `badjuju.fetch.run` | jj: Git fetch | Run `jj git fetch` |
| `badjuju.push.normal` | jj: Git push | Run `jj git push` |
| `badjuju.push.forceWithLease` | jj: Git push --force-with-lease | Force push with lease |
| `badjuju.edit.cursor` | jj: Edit commit at cursor | Move @ to commit under cursor |
| `badjuju.abandon.cursor` | jj: Abandon commit at cursor | Abandon commit under cursor |
| `badjuju.diff.cursor` | jj: Show diff for commit at cursor | Diff for commit under cursor |
| `badjuju.describe.finalize` | jj: Finalize commit description | Save and close describe buffer |
| `badjuju.squash.file` | jj: Squash file at cursor | Move file under cursor into parent |
| `badjuju.unsquash.file` | jj: Unsquash file at cursor | Pull file back from parent |
| `badjuju.rebase.prompt` | jj: Rebase to destination | Prompt and rebase |
| `badjuju.bookmark.prompt` | jj: Bookmark | Interactive bookmark manager |
| `badjuju.log.applyShortcut` | jj: Apply revset shortcut | Follow revset link in log |
| `badjuju.help.open` | jj: Show hotkey help | Cheat sheet for current buffer |
| `badjuju.version.open` | jj: Show version | Server and jj versions |
| `badjuju.restartLanguageServer` | jj: Restart Language Server | Restart the LSP |

## Keymap profiles

Set `badjuju.keymapProfile` to choose:

- `"magit"` (default) — single-key bindings
- `"vim"` — two-letter verb chords
- `"none"` — no built-in keymaps

### `magit` profile — selected bindings

#### `status.jujutsu` / `log.jujutsu`

| Key | Action |
| --- | ------ |
| `R` | Refresh |
| `n` | New commit |
| `L` | Open log |
| `Ctrl+N` / `Ctrl+P` | Move forward / back (`jj next` / `jj prev`) |
| `Ctrl+Shift+N` / `Ctrl+Shift+P` | Edit next / previous in place |
| `f` / `p` / `P` | Fetch / push / force push |
| `e` | Edit commit at cursor |
| `b` | Bookmark |
| `r` | Rebase |
| `d` | Diff (change) |
| `D` | Diff (commit, pinned) |
| `c w` | Commit transient → reword (describe) commit at cursor |
| `c n` | Commit transient → new child commit |
| `s` | Squash file at cursor |
| `u`, `Ctrl+K u` | Unsquash file at cursor |
| `a` | Abandon |
| `U` | Undo |
| `=` | Diff (alias for `d`) |
| `q` | Close |
| `?` | Help |

#### Squash window (`.jj/badjuju/squash/*.jujutsu`)

| Key | Action |
| --- | ------ |
| `s` | Toggle hunk / file between SELECTED and REMAINING |
| `e` | Edit hunk before squashing |
| `a` | Select all changes |
| `A` | Deselect all changes |
| `u` | Undo |
| `Tab` | Toggle fold |
| `q` | Close |

#### `describe.jujutsu`

| Key | Action |
| --- | ------ |
| `Ctrl+Enter` | Finalize commit (save and close) |
| `Escape Escape` | Abort (close without saving) |
| `?` | Help |

### `vim` profile

Doubled letters: `nn`, `ll`, `dd`, `ss`, `uu`, `UU`, `aa`, `ee`,
`bb`, `rr`, `ff`, `pp`, `PP`. Single-key bindings: `D` (diff commit),
`=` (diff change, alias for `D`'s change-mode sibling), `q`, `?`. See
the in-repo `clients/vscode/README.md` for the complete table.

## Settings

| Setting | Purpose |
| ------- | ------- |
| `badjuju.binaryPath` | Path to the `jj` binary; blank uses `PATH` |
| `badjuju.defaultLogRevset` | Default revset for `badjuju.log.open` |
| `badjuju.keymapProfile` | `magit`, `vim`, or `none` |

## Auto-refresh

Open status, log, and diff buffers auto-refresh when `jj` operations
happen — including ones triggered from a terminal outside VS Code.
