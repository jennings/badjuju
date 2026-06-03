# Bad Juju — Helix Client

Helix has no plugin system, so this directory provides a `languages.toml`
snippet to merge into your config plus a walkthrough for working with
Jujutsu through the LSP.

## Requirements

- Helix 25.01+
- `jj` on your `PATH`
- `badjuju` binary on your `PATH`

## Setup

### 1. Install badjuju

From a checkout of the bad-juju repo:

```sh
redo server/install   # installs badjuju to ~/.cargo/bin/
```

> No `redo` installed? Run `./do server/install` instead — the repo ships
> a self-contained `./do` shell script as a drop-in fallback. Every `redo
> <target>` below works as `./do <target>`.

Make sure `~/.cargo/bin` is on your `PATH`.

### 2. Merge the language config

Copy the contents of `languages.toml` in this directory into your Helix
language config. You have two options:

**User-wide** — merge into `~/.config/helix/languages.toml`:

```sh
cat clients/helix/languages.toml >> ~/.config/helix/languages.toml
```

**Per-project** — create or append to `.helix/languages.toml` at the root of
your project (Helix merges project config on top of user config).

### 3. Open a buffer

The canonical one-liner opens the working-copy status view in Helix:

```sh
hx "$(badjuju status)"
```

Similarly for the log view, and for diff views:

```sh
hx "$(badjuju log)"
hx "$(badjuju diff)"            # change diff for @ (updates on amend)
hx "$(badjuju diff --revision abc123)"   # diff for a specific revision
```

### Multiple diffs

You can open multiple diffs simultaneously — each has a unique filename based on its revision id:

```sh
# Open two change diffs side by side (each updates independently on amend)
hx "$(badjuju diff --revision abc)" "$(badjuju diff --revision def)"
```

Files are named `diff-change-<12char-id>.jujutsu` (for change diffs, which update
when the change is amended) or `diff-commit-<12char-id>.jujutsu` (for commit diffs,
pinned to a specific immutable commit that never changes).

## Navigation

Once a `*.jujutsu` buffer is open, the LSP provides code actions on commit
lines. Press `Space a` to see available actions for the commit under the
cursor.

### Commit-row code actions

When the cursor is on a commit header row in `status.jujutsu` or `log.jujutsu`:

| Action | Description |
|--------|-------------|
| Edit commit `<rev>` | `jj edit <rev>` — move `@` to this commit |
| Abandon commit `<rev>` | `jj abandon <rev>` |
| Describe commit `<rev>` | Open `describe.jujutsu` for editing the commit message |
| Show diff for `<rev>` | Write `diff-change-<id>.jujutsu` (change mode, updates on amend) |
| New child of `<rev>` | `jj new <rev>` |
| Rebase commit `<rev>`… | `jj rebase -r <rev>` |
| Bookmark `<rev>`… | Bookmark management menu |
| Squash from this revision | Mark this commit as the squash source; status/log header updates to confirm |

Once a squash source is marked, the following actions appear on every commit row:

| Action | Description |
|--------|-------------|
| Squash into this revision | Use this commit as the destination; materializes the squash window |
| Cancel pending squash | Clear the pending squash source without performing any operation |

### File-row code actions

When the cursor is on a changed-file line in `status.jujutsu`:

| Action | Description |
|--------|-------------|
| Squash `<file>` | Move this file from `@` into its parent |
| Unsquash `<file>` | Pull this file from the parent back into `@` |

### Squash-window code actions

When the squash window (`.jj/badjuju/squash/<from>-<to>.jujutsu`) is open:

| Action | Description |
|--------|-------------|
| Move hunk to SELECTED | Move the hunk under the cursor into the SELECTED CHANGES section |
| Move hunk to REMAINING | Move the hunk under the cursor back into the REMAINING CHANGES section |
| Move file `<name>` to SELECTED | Move all hunks for a file into SELECTED CHANGES |
| Move file `<name>` to REMAINING | Move all hunks for a file back into REMAINING CHANGES |
| Move all hunks to SELECTED | Select every change (equivalent to a plain `jj squash`) |
| Move all hunks to REMAINING | Deselect all changes |

## Commit-to-commit squash walkthrough

To move specific hunks from one commit into another:

1. Open the status or log view and place the cursor on the **source** commit
   (the one whose changes you want to move). Press `Space a` and choose
   **Squash from this revision**. The status/log header updates to show the
   pending source.

2. Move the cursor to the **destination** commit (the one you want to receive
   the changes). Press `Space a` and choose **Squash into this revision**. The
   server writes the squash window to disk and returns its path.

3. Open the squash window if Helix does not do so automatically:

   ```
   :open .jj/badjuju/squash/<from>-<to>.jujutsu
   ```

4. Navigate to a hunk or file header in the **REMAINING CHANGES** section.
   Press `Space a` and choose **Move hunk to SELECTED** (or **Move file …**).
   The buffer refreshes immediately to reflect the new state.

5. Repeat step 4 until all desired changes are in SELECTED CHANGES. When
   finished, close the squash buffer — the operation is applied automatically
   when the server finalizes the selection. If you want to move everything at
   once, use **Move all hunks to SELECTED** from any line.

## Log shortcut actions

In `log.jujutsu`, lines beginning with `JJ:` are named revset shortcuts. Place
the cursor on a `JJ:` line and press `Space a` → **Apply revset: `<label>`** to
re-run the log query with that revset.

## Syntax highlighting

Syntax highlighting comes from LSP semantic tokens, which the server
advertises automatically. No Helix grammar or tree-sitter query is required.

## Known limitations

After an action that produces a new buffer (e.g. Show diff, Squash into this
revision), Helix does not auto-open the returned file path. Open it manually:

```
:open .jj/badjuju/diff-change-<id>.jujutsu
:open .jj/badjuju/squash/<from>-<to>.jujutsu
```

(The exact filename is printed by the corresponding `badjuju` CLI command or
returned by the code action.)

## Auto-reload

When a `jj` operation occurs outside the editor (e.g. you run `jj new` in a
terminal), the server detects the op-head change and pushes the refreshed
status, log, and diff content to Helix via `workspace/applyEdit`. Open
buffers update in place without a manual `:reload`.

Helix's apply-edit handler currently marks the buffer modified after a
server-driven edit. The on-disk file already matches, so `:write` is a
no-op (or you can ignore the modified indicator until the next real edit).
