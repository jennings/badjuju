# Other Editors

Bad Juju is "just" an LSP server, which means any editor with a
sufficiently capable LSP client can drive it. The first-party Helix
configuration is documented here as the canonical example; the same
principles apply to other LSP-capable editors.

## Helix

Helix has no plugin system, so the integration is a small
`languages.toml` snippet plus a few CLI one-liners.

### Requirements

- Helix 25.01+
- `jj` and `badjuju` on your `PATH`

### Setup

1. **Install the server.** See [Getting
   Started](../getting-started.md#1-install-the-server).
2. **Merge the language config.** Copy `clients/helix/languages.toml`
   from the repo into either:
   - `~/.config/helix/languages.toml` (user-wide), or
   - `.helix/languages.toml` at the root of your project
     (per-project).

### Opening buffers

Helix doesn't auto-open files returned by code actions, so the entry
point is the shell:

```sh
hx "$(badjuju status)"
hx "$(badjuju log)"
hx "$(badjuju diff)"                       # change diff for @
hx "$(badjuju diff --revision abc123)"     # diff a specific revision
```

You can open multiple diffs at once:

```sh
hx "$(badjuju diff --revision abc)" "$(badjuju diff --revision def)"
```

### Navigation

Once a `.jujutsu` buffer is open, Helix uses **`Space a`** for code
actions. With the cursor on a commit row you'll get:

| Action | Description |
| ------ | ----------- |
| Edit commit `<rev>` | Move `@` to this commit |
| Abandon commit `<rev>` | Delete this commit |
| Describe commit `<rev>` | Edit commit message |
| Show diff for `<rev>` | Open the change diff |
| New child of `<rev>` | `jj new <rev>` |
| Rebase commit `<rev>`… | Prompts for destination |
| Bookmark `<rev>`… | Bookmark management menu |
| Squash from this revision | Mark this commit as squash source |

After marking a source, the **Squash into this revision** and
**Cancel pending squash** actions appear on every commit row. See
[Manipulating Commits](../usage/manipulating-commits.md#squashing-changes-between-revisions)
for the full walkthrough.

For file rows in the status buffer:

| Action | Description |
| ------ | ----------- |
| Squash `<file>` | Move file from `@` into parent |
| Unsquash `<file>` | Pull file from parent back into `@` |
| Log `<file>` | Open the [log file buffer](../reference/log-file-buffer.md) for this path |

The **Log `<file>`** action opens
`.jj/badjuju/file/<path>.jujutsu` on demand. Helix doesn't auto-open
files returned by code actions, so open it manually after invoking
the action.

For squash-window rows:

| Action | Description |
| ------ | ----------- |
| Move hunk to SELECTED / REMAINING | Toggle the hunk under the cursor |
| Move file `<name>` to SELECTED / REMAINING | Toggle a whole file |
| Move all hunks to SELECTED / REMAINING | Select / deselect everything |

### Log shortcuts

In `log.jujutsu`, lines beginning with `JJ:` are revset shortcuts.
Place the cursor on one and choose **Apply revset: `<label>`** from
`Space a` to re-run the log with that revset.

> Other clients bind `Enter`/`RET` to a context-aware dispatch — on
> shortcut lines it applies the revset, elsewhere it falls through
> to go-to-definition. Helix has no keybinding layer in Bad Juju,
> so use `Space a` for both.

### Auto-reload

When a `jj` operation runs outside Helix, the server pushes the
refreshed buffer content via `workspace/applyEdit`. Helix's handler
marks the buffer modified after the edit even though the on-disk
file already matches; `:write` is then a no-op. You can ignore the
modified indicator until your next real edit.

### Known limitations

Helix doesn't auto-open files returned by code actions. If
**Show diff for `<rev>`** or **Squash into this revision** report a
new file path, open it manually:

```
:open .jj/badjuju/diff-change-<id>.jujutsu
:open .jj/badjuju/squash/<from>-<to>.jujutsu
```

## Any other LSP-capable editor

If your editor:

- Speaks the Language Server Protocol,
- Can launch a stdio LSP server with a custom command, and
- Can invoke code actions (`textDocument/codeAction`),

…then it can drive Bad Juju. You'll need to wire up:

1. A filetype for `*.jujutsu` files.
2. An LSP server config that launches `badjuju lsp` and detects the
   workspace via a `.jj/` marker.
3. (Optional) Keybindings or commands that send
   `workspace/executeCommand` for the `badjuju.*` operations listed in
   the [Status buffer reference](../reference/status-buffer.md) and
   elsewhere.

Reading the [VS Code extension
source](https://github.com/jennings/badjuju/tree/main/clients/vscode/src)
and the [Helix `languages.toml`
snippet](https://github.com/jennings/badjuju/blob/main/clients/helix/languages.toml)
will give you a working template. If you build an integration for a
new editor, please [open an
issue](https://github.com/jennings/badjuju/issues) — we'd love to
include it.
