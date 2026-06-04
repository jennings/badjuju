# Manipulating Commits

Once you can navigate the status window, the next step is rewriting
history. Jujutsu is built around the assumption that commits are
mutable — you reword them, split them, squash hunks between them, and
move them around. Bad Juju exposes those operations as cursor-driven
actions on the status and log buffers.

This page walks through the most common ones.

## Abandoning a change

If you decide a commit shouldn't exist at all — say you started a
spike, hated it, and want it gone — **abandon** it. The commit's
descendants get rebased onto its parent automatically.

1. Open the status or log buffer.
2. Place the cursor on the commit you want to delete.
3. Press `a` (magit profile in Neovim/VS Code/Emacs) or invoke the
   **Abandon commit `<rev>`** code action.

Bad Juju runs `jj abandon <rev>` and refreshes the buffer. With no
cursor on a commit row, `a` defaults to the working copy.

> **Heads up:** Jujutsu's `op log` makes this reversible. If you
> abandoned the wrong thing, press `u` (or `U` in Emacs) to invoke
> `jj undo`.

## Rewording a commit (describe)

If the commit is fine but the message isn't — typo, missing context,
wrong issue number — you want to **describe** it.

1. Place the cursor on the commit.
2. Press `d`.

Bad Juju opens `describe.jujutsu` populated with the existing message.
Lines beginning with `JJ:` are comments that get stripped on save.
Save and close (`<C-c><C-c>`, `Ctrl+Enter`, or `C-c C-c` depending on
the editor) to apply.

If you change your mind, abort instead of saving (`<C-c><C-k>`,
`Escape Escape`, or `C-c C-k`).

## Squashing a single file into the parent

Suppose you realize a change you made belongs in the previous commit,
not in your current working copy. **Squashing** a file moves its
changes from `@` (the working copy) into the parent commit.

1. Open the status buffer.
2. Place the cursor on the file in the **WORKING COPY CHANGES** list.
3. Press `s`.

The file disappears from `@`'s changes and lands in the parent. If
the working copy ends up empty, you can keep working on the same
commit or move on with `n` (new).

Need to pull a file *back out* of the parent? Press `U` (or `Ctrl+K U`
in VS Code if `U` is shadowed by another keymap) — that's `unsquash`.

> **Multiple parents?** If the working copy is a merge commit, Bad
> Juju will pick the parent that already touches the file. If the file
> isn't in either parent (or both), the client prompts you to pick
> one.

## Squashing changes between revisions

Suppose you decide a change should be moved to a *different* revision
— not just the immediate parent, and maybe only some of the hunks.
Bad Juju's commit-to-commit squash workflow handles this.

The flow has three steps: mark a **source**, mark a **destination**,
then pick the hunks.

### 1. Mark the source

Place your cursor on the commit whose changes you want to move *out*
of. Press `s` (Emacs) or the **Squash from this revision** code action
(VS Code, Neovim, Helix).

The status/log buffer header updates to confirm the pending source.

### 2. Mark the destination

Now move the cursor to the commit you want the changes to land *in*.
Press `s` again (Emacs) or invoke **Squash into this revision**.

Bad Juju materializes a **squash window** at
`.jj/badjuju/squash/<from>-<to>.jujutsu`. It has two sections:

- **REMAINING CHANGES** — every hunk in the source that hasn't been
  selected yet.
- **SELECTED CHANGES** — the hunks you're moving into the
  destination.

Initially, everything is in REMAINING.

### 3. Pick the hunks

In the squash window, navigate to a hunk and toggle it between
REMAINING and SELECTED:

| Client | Key |
| ------ | --- |
| Neovim, VS Code | `s` (toggle hunk/file under cursor) |
| Emacs | `s` |
| Helix | code action **Move hunk to SELECTED** / **Move hunk to REMAINING** |

You can also:

- **Select everything** with `a` (Emacs) or **Move all hunks to
  SELECTED** — equivalent to a plain `jj squash`.
- **Deselect everything** with `A` (Emacs) or **Move all hunks to
  REMAINING**.
- **Edit a hunk before squashing.** Press `e` (Emacs) on a hunk to
  open `hunk-edit.jujutsu`. You can tweak the additions and context
  lines, save, and Bad Juju applies the edited hunk via `jj squash
  --interactive`. See the [Hunk edit buffer
  reference](../reference/hunk-edit-buffer.md) for details.

### 4. Finalize

Close the squash window when you're happy with the SELECTED set. Bad
Juju applies the move and refreshes the status/log buffers. If you
change your mind partway through, cancel the pending squash via the
**Cancel pending squash** action (or just close the squash window
with everything still in REMAINING — nothing gets moved).

## Rebasing onto a different destination

To move a commit (and its descendants) onto a new parent, press `r`
with the cursor on the commit you want to rebase. The client prompts
for a destination revset; on submit, Bad Juju runs `jj rebase -r <src>
-d <dest>`.

## Managing bookmarks

Press `b` to open the bookmark manager. It's a single popup/picker
that handles create, move, delete, track, and forget operations — the
same five things `jj bookmark` does, just from inside your editor.

## Undoing the last operation

Jujutsu records every mutating operation in its **op log**, which
makes mistakes cheap. To undo whatever you just did:

| Client | Key |
| ------ | --- |
| Neovim, VS Code (magit) | `u` |
| Emacs | `U` |
| Helix | code action via shell `jj undo` |

This invokes `jj undo` and refreshes the status buffer.

## Pulling from / pushing to the Git remote

If your repo is colocated with Git, you can fetch and push without
leaving the editor:

| Action | Key |
| ------ | --- |
| `jj git fetch` | `f` |
| `jj git push` | `p` |
| `jj git push --force-with-lease` | `P` |

These are best-effort wrappers — they run the corresponding `jj`
command and surface the output. For anything more nuanced (specific
remotes, branches, or refspecs), drop into the terminal.
