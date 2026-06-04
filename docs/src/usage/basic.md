# Basic Usage

This page covers the everyday actions: looking at your working copy,
starting a new change, editing a commit message, and viewing a diff.
The examples use the default `magit` keymap that ships with each
client — if you've rebound them, substitute your own keys.

## Opening the status window

The status window is your home base. It shows what files you've
touched, the stack of commits leading up to `@` (the working copy),
and a one-screen command reference at the bottom.

To open it:

| Client | How |
| ------ | --- |
| Neovim | `:JJStatus` |
| VS Code | Command Palette → `jj: Status` |
| Emacs | `M-x badjuju-status` |
| Helix | `hx "$(badjuju status)"` from the shell |

You'll see something like:

```
STATUS:

The working copy has no changes.
Working copy  (@) : kpkzwvqm 909679d0 (empty) (no description set)
Parent commit (@-): xorwskru 66bfbfdf feat(neovim): buffer-local keymaps...

STACK: ancestors(reachable(@, mutable()), 2)

@  kpkzwvqm 909679d0 1min stephen@example.com
│  (empty) (no description set)
○  xorwskru 66bfbfdf 2min stephen@example.com
│  feat(neovim): buffer-local keymaps on status.jj and log.jj
◆  spxlzwpr 18d66a82 20min stephen@example.com main
│  fix(ci): set DESTDIR when installing redo
~

COMMAND REFERENCE:
n     new change
L     open log
d     describe
...
```

Once the buffer is open, single-key bindings (or `M-x` / Command
Palette equivalents) drive every other action. Press `?` at any time
to see the active key map for the current buffer.

## Creating a new revision

You finished a commit and want to start working on the next thing?
That's `jj new`, but you don't need to leave the editor:

| Client | Key |
| ------ | --- |
| Neovim, VS Code (magit) | `n` |
| Emacs | `n` |
| Helix | code action **New child of `<rev>`** |

`n` runs `jj new` against the working copy. If you'd rather branch off
a different commit, place your cursor on that commit's header line
first — in clients with hotkeys, the cursor position is what
determines the target.

## Editing an existing revision

There are two flavors of "edit" in Jujutsu:

- **Move `@` to a commit so you can keep editing its working tree.**
  Press `e` (or run `:JJEdit` / `M-x badjuju-edit`) with the cursor on
  the commit you want to land on. Bad Juju runs `jj edit <rev>` and
  refreshes the status window.

- **Change a commit's *message* without touching its tree.** That's
  `describe`. Press `d` with the cursor on the commit; Bad Juju opens a
  `describe.jujutsu` buffer pre-populated with the current message.
  Edit it, save, and Bad Juju calls `jj describe -m` for you.

In the describe buffer:

| Client | Finalize | Abort |
| ------ | -------- | ----- |
| Neovim | `<C-c><C-c>` | `<C-c><C-k>` |
| VS Code | `Ctrl+Enter` | `Escape Escape` |
| Emacs | `C-c C-c` | `C-c C-k` |
| Helix | `:write` then `:quit` | `:quit!` |

Lines starting with `JJ:` are comments — they're stripped before the
message is saved, so you can leave reminders in them.

## Viewing a diff

There are two diff modes:

- **Change diff** — pinned to a *change id* (the stable identifier that
  follows a commit as you amend it). The diff buffer refreshes
  automatically when the change is amended. This is what you want most
  of the time.
- **Commit diff** — pinned to an immutable *commit id*. The view never
  changes; useful for "what did this exact snapshot look like?"

To open a change diff, place the cursor on a commit and press `D`. In
Emacs, that's `D` from the status or log buffer; in VS Code/Neovim
magit, also `D`. To open a commit diff in VS Code, use
`Ctrl+Shift+D`.

You can have multiple diff buffers open simultaneously — each one is a
separate file (`diff-change-<id>.jujutsu` or
`diff-commit-<id>.jujutsu`), so you can compare two revisions side by
side.

## Refreshing and closing buffers

Bad Juju auto-refreshes open status, log, and diff buffers when a `jj`
operation happens — whether you triggered it through Bad Juju or
through `jj` in a terminal. You should rarely need to refresh
manually, but if you want to:

- Press `R` (Neovim, VS Code, Emacs) in the buffer.
- Or run `:JJRefresh` / `M-x badjuju-refresh` / the command-palette
  refresh action.

To close a buffer: press `q`. In Helix, use `:bd` (buffer close).

## What's next

You've got the basics. Up next:

- [**Manipulating Commits**](./manipulating-commits.md) covers
  abandoning, rewording, and squashing — the operations that actually
  rewrite history.
- The [**Reference**](../reference/index.md) chapter has the full
  catalog of each buffer and the keys it responds to.
