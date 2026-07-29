# Bad Juju — Helix (Steel) Client

An optional enhancement to the [plain LSP-only Helix setup](../helix/README.md)
for people running a **Steel-enabled Helix build**. It adds:

- A named command for every `badjuju.*` server command (`:jj-status`,
  `:jj-new`, `:jj-squash`, …), so you don't need `hx "$(badjuju status)"`
  shell one-liners or `Space a` for everyday actions.
- Auto-open: when a command writes a new file (a fresh `describe.jujutsu`, a
  diff, a squash window), this plugin opens it — vanilla Helix's built-in
  code-action UI discards that result (see "Why this exists" below).
- `RET` context dispatch in `log.jujutsu`: apply a `JJ:` revset shortcut on a
  shortcut line, fall through to go-to-definition everywhere else — matching
  the Neovim/Emacs clients.
- An optional `tab`-prefixed keymap menu for single-letter access to the same
  commands, styled after the other clients' magit profile.

Everything else (syntax highlighting, semantic tokens, diagnostics,
goto-definition on commit/file rows, live auto-reload via
`workspace/applyEdit`) already works with the [plain `languages.toml`
setup](../helix/README.md) and needs none of this.

## Status: unmerged, experimental Helix fork required

**Steel is not in mainline Helix.** [`helix-editor/helix#8675`](https://github.com/helix-editor/helix/pull/8675)
(open since October 2023, labeled `S-experimental`) is still under review.
Using this plugin means building Helix yourself from
[`mattwparas/helix`](https://github.com/mattwparas/helix), branch
`steel-event-system`, with the `steel` Cargo feature enabled — not the
release binary you'd get from Homebrew, your distro, or the Helix website.

Concretely, this means:

- No CI in this repo builds or tests against a Steel-enabled `hx`. The tests
  under `test/` (see "Testing" below) exercise the plugin's own logic
  against a plain `steel` interpreter with the `helix/*` modules stubbed
  out — they prove the Scheme is correct, not that it behaves correctly
  inside a real Helix process.
- The Steel API surface (`helix-term/src/commands/engine/steel/mod.rs`) is
  pre-1.0 and can rename or remove functions between commits on that branch.
  If something in this plugin breaks after rebuilding your fork, check
  `STEEL.md` and `steel-docs.md` in your checkout for what changed.
- If you'd rather not build a fork at all, the [plain `languages.toml`
  setup](../helix/README.md) works on stock Helix 25.01+ today and covers
  the LSP-driven parts (syntax highlighting, diagnostics, code actions,
  goto-definition) with no compromises other than manual `:open` after
  state-changing actions and no `RET` dispatch.

## Requirements

- A Steel-enabled Helix build (see below)
- `jj` on your `PATH`
- `badjuju` binary on your `PATH`

## Setup

### 1. Build Steel-enabled Helix

```sh
git clone https://github.com/mattwparas/helix.git
cd helix
git checkout steel-event-system
cargo xtask steel
```

This installs the `hx` executable (with Steel support compiled in), the
`steel` REPL/CLI, the Steel language server, and `forge` (Steel's package
manager). Full instructions: `STEEL.md` in that checkout.

### 2. Install badjuju

From a checkout of this repo:

```sh
redo server/install   # or: ./do server/install
```

Make sure `~/.cargo/bin` is on your `PATH`.

### 3. Merge the language config

Same file the plain setup uses — copy `clients/helix/languages.toml`'s
contents into `~/.config/helix/languages.toml` or `.helix/languages.toml`:

```sh
cat clients/helix/languages.toml >> ~/.config/helix/languages.toml
```

### 4. Install the plugin files

Copy this directory's `cogs/badjuju.scm` and `cogs/badjuju-core.scm` into
your Steel config's `cogs/` directory:

```sh
mkdir -p ~/.config/helix/cogs
cp clients/helix-steel/cogs/badjuju.scm clients/helix-steel/cogs/badjuju-core.scm \
   ~/.config/helix/cogs/
```

### 5. Wire it into `helix.scm` and `init.scm`

`badjuju.scm`'s functions must be re-`provide`d from your `helix.scm` before
Helix will recognize them as typed commands (this is how every Steel plugin
works — see `STEEL.md`'s own `git-blame` example):

```scheme
;; ~/.config/helix/helix.scm
(require "cogs/badjuju.scm")

(provide jj-status jj-log jj-log-file jj-describe jj-diff jj-diff-commit
         jj-new jj-next jj-prev jj-refresh jj-squash jj-squash-commit
         jj-squash-toggle jj-squash-edit-hunk jj-squash-select-all
         jj-squash-select-none jj-unsquash jj-undo jj-abandon jj-edit
         jj-fetch jj-push jj-push-force jj-rebase-onto jj-rebase-after
         jj-rebase-before jj-cancel jj-bookmark-create jj-bookmark-move
         jj-bookmark-delete jj-bookmark-track jj-bookmark-forget jj-help
         jj-keymap jj-version jj-ret jj-code-action jj-key-s jj-key-a
         jj-key-u)
```

Then install the keymap from `init.scm`:

```scheme
;; ~/.config/helix/init.scm
(jj-install-keymap!)
```

## Commands

Every command below is available from the command palette (`:jj-status`,
etc.) once wired up per step 5. Run `:jj-status` first — from any buffer
inside a `jj` workspace, it starts the LSP client on demand.

| Command | Description |
|---|---|
| `:jj-status` | Open the working-copy status buffer |
| `:jj-log [revset]` | Open the commit log |
| `:jj-log-file [revset]` | Per-file history for the file at cursor (status) or current buffer |
| `:jj-describe [revision]` | Edit a commit message |
| `:jj-diff [revision]` | Change diff (updates on amend) |
| `:jj-diff-commit [revision]` | Pinned commit diff |
| `:jj-new [parent]` | Create a new change |
| `:jj-next` / `:jj-prev` | Move `@` to next child / previous parent |
| `:jj-refresh` | Refresh the current badjuju buffer |
| `:jj-squash` | Squash file/working-copy line at cursor into parent |
| `:jj-squash-commit` | Two-step commit-to-commit squash (mark source, then destination) |
| `:jj-squash-toggle` | In a squash window: toggle hunk/file at cursor |
| `:jj-squash-edit-hunk` | In a squash window: edit the hunk at cursor before squashing |
| `:jj-squash-select-all` / `:jj-squash-select-none` | Move every hunk to SELECTED / REMAINING |
| `:jj-unsquash` | Unsquash file at cursor from parent into it |
| `:jj-undo` | `jj undo` |
| `:jj-abandon [revision]` | Abandon a revision |
| `:jj-edit [revision]` | Move `@` to a revision |
| `:jj-fetch` | `jj git fetch` |
| `:jj-push` / `:jj-push-force` | `jj git push` (with/without `--force-with-lease`) |
| `:jj-rebase-onto` / `:jj-rebase-after` / `:jj-rebase-before` | Complete a pending rebase (mark source first via a code action) |
| `:jj-cancel` | Cancel a pending squash or rebase |
| `:jj-bookmark-create` / `-move` / `-delete` / `-track` / `-forget` `<name>` | Bookmark management |
| `:jj-help [window]` | Command reference for a window kind, in a scratch buffer |
| `:jj-keymap` | Active keymap profile, in a scratch buffer |
| `:jj-version` | Server version, in a scratch buffer |

Commands taking a revision/parent argument use the cursor position when
invoked from `status.jujutsu`/`log.jujutsu` with no argument, matching the
other clients' cursor-form dispatch.

## Keybindings

Deliberately minimal at the top level — see the extensive comment in
`cogs/badjuju.scm` above `jj-install-keymap!` for the full reasoning. Short
version: Helix's normal mode uses single letters as core editing primitives
(`d` = delete, `u` = undo, `p` = paste, `q` = record macro, `?` = reverse
search, …), and `describe.jujutsu`/`hunk-edit.jujutsu` are real text-entry
buffers, not read-only views — a flat magit-style letter scheme (the kind
Kakoune/Neovim/Emacs ship, gated by a leader key or a true per-buffer local
map) would silently break editing there, since Steel's extension/label
keymap has no per-window-kind fallback the way those clients' keying
mechanisms do.

`jj-install-keymap!` therefore only binds:

- **`RET`** — context dispatch (`jj-ret`): applies a `JJ:` revset shortcut on
  a shortcut line in `log.jujutsu`, otherwise goto-definition (a no-op on
  prose, so this is safe in `describe`/`hunk-edit` too). No default Helix
  normal-mode binding exists for bare Enter.
- **`tab`** — opens a "Bad Juju" sub-menu (the same mechanism as Helix's own
  `g`/`z`/`space` prefixes — press it and a popup lists the bound keys and
  their descriptions). `tab` has no normal-mode binding in vanilla Helix
  either (insert-mode only), so claiming it as a prefix costs nothing.

| `tab` then… | Action |
|---|---|
| `n` | New change |
| `l` / `L` | Open log / per-file log |
| `d` / `D` | Diff (change / commit) |
| `e` | Edit commit at cursor |
| `s` | Squash-commit (status/log) or squash-toggle (squash window) |
| `S` | Squash file at cursor |
| `a` | Abandon revision (status/log) or select-all (squash window) |
| `u` | Unsquash (status) or undo (squash window) |
| `U` | `jj undo` |
| `f` / `p` / `P` | Fetch / push / push --force-with-lease |
| `R` | Refresh |
| `x` | Cancel pending squash/rebase |
| `q` | Close buffer |
| `A` | Native code-action picker |
| `?` | Help (command reference for the current window) |

Want the full flat scheme anyway? Bind the letters yourself, scoped to a
window kind — `jj-window-kind` (from `badjuju-core.scm`) is exported for
exactly this.

## Why this exists

Two gaps the plain `languages.toml` setup can't close on its own:

1. **Helix's native code-action UI discards command results.**
   `helix-view::Editor::execute_lsp_command` fires
   `workspace/executeCommand` and only logs errors on the response — by
   design, Helix expects state changes to arrive via `workspace/applyEdit`
   instead. badjuju communicates back by *returning* the URI of the file it
   just wrote (a `describe.jujutsu`, a diff, a squash window), which only a
   caller that reads the JSON-RPC result can act on. `Space a` therefore
   never auto-opens anything (documented in the [plain setup's known
   limitations](../helix/README.md#known-limitations)) — you have to
   `:open` the returned path by hand. `jj-execute!` in `badjuju.scm` fixes
   this by opening the result itself.
2. **Helix has no keybinding layer without Steel.** There's no way to bind
   `RET` to "apply revset shortcut here, goto-definition there" the way
   Neovim/Emacs do, and no way to give `.jujutsu` buffers dedicated
   keybindings at all — `Space a` was the only path to any badjuju action.

## Testing

```sh
redo clients/helix-steel/test   # or: ./do clients/helix-steel/test
```

Two layers, both run against a plain `steel` interpreter — no Steel-enabled
`hx` build required or used:

- `test/badjuju-test.scm`: unit tests for the pure logic in
  `cogs/badjuju-core.scm` (window classification, `JJ:` shortcut-line
  detection, `file://` URI stripping, cursor-argument construction). Real
  assertions against the real file, no stubbing.
- `test/smoke.scm`: loads the real `cogs/badjuju.scm` against stub
  `helix/*` modules (`test/fixtures/helix/`) and calls every exported
  command across each window kind the dispatch logic branches on
  (status/log/squash/describe). Catches load-time errors (bad `require`
  paths, a module import missing) and call-time errors (arity mismatches,
  `(void)` vs. `void`) that the unit tests can't reach since they never load
  `badjuju.scm` itself.

What this does **not** prove: that the plugin behaves correctly inside a
real Steel-enabled Helix (actual rope/selection semantics, real LSP
round-trips, real keymap-registration effects, the `tab` popup rendering
correctly). No such build exists in this repo's CI. If you're changing this
plugin, test the change by hand against your own Steel-Helix build before
relying on `redo test` alone.
