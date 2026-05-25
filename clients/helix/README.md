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

Similarly for the log and diff views:

```sh
hx "$(badjuju log)"
hx "$(badjuju diff)"
```

## Navigation

Once a `*.jujutsu` buffer is open, the LSP provides code actions on commit
lines. Press `Space a` to see available actions for the commit under the
cursor:

- **Edit** — `jj edit <rev>`
- **New child** — `jj new <rev>`
- **Describe** — opens `describe.jujutsu` for editing the commit message
- **Show diff** — writes `diff.jujutsu` for the commit

## Syntax highlighting

Syntax highlighting comes from LSP semantic tokens, which the server
advertises automatically. No Helix grammar or tree-sitter query is required.

## Known limitations

After an action that produces a new buffer (e.g. Show diff), Helix does not
auto-open the returned file path. Open it manually:

```
:open .jj/badjuju/diff.jujutsu
```

Subsequent saves to `describe.jujutsu` are handled by `textDocument/didSave`
and the buffer refreshes on disk, but Helix requires a manual `:reload` to
pick up the change in the already-open status or log buffer.
