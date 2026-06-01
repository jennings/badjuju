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
cursor:

- **Edit** — `jj edit <rev>`
- **New child** — `jj new <rev>`
- **Describe** — opens `describe.jujutsu` for editing the commit message
- **Show diff** — writes `diff-change-<id>.jujutsu` (change mode, updates on amend)

## Syntax highlighting

Syntax highlighting comes from LSP semantic tokens, which the server
advertises automatically. No Helix grammar or tree-sitter query is required.

## Known limitations

After an action that produces a new buffer (e.g. Show diff), Helix does not
auto-open the returned file path. Open it manually:

```
:open .jj/badjuju/diff-change-<id>.jujutsu
```

(The exact filename is printed by `badjuju diff`.)

## Auto-reload

When a `jj` operation occurs outside the editor (e.g. you run `jj new` in a
terminal), the server detects the op-head change and pushes the refreshed
status, log, and diff content to Helix via `workspace/applyEdit`. Open
buffers update in place without a manual `:reload`.

Helix's apply-edit handler currently marks the buffer modified after a
server-driven edit. The on-disk file already matches, so `:write` is a
no-op (or you can ignore the modified indicator until the next real edit).
