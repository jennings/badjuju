# Hunk edit buffer

The hunk edit buffer lives at `.jj/badjuju/hunk-edit.jujutsu`. It's
opened from inside a squash window when you want to tweak the
contents of a single hunk before squashing it into the destination
commit.

## When you'd use it

You're in a commit-to-commit squash (see [Manipulating
Commits](../usage/manipulating-commits.md#squashing-changes-between-revisions)).
You've picked a hunk to move from the source commit into the
destination, but a couple of lines in that hunk don't actually belong
in the destination. Rather than splitting the hunk by hand or moving
the whole thing and re-editing afterward, you edit it once, at
selection time.

## How it works

1. From the squash window, place the cursor on a hunk header.
2. Press `e` (Emacs) or invoke the **Edit hunk** code action.
3. Bad Juju opens `hunk-edit.jujutsu` populated with the hunk's
   contents:

   ```
   JJ: Editing hunk in <file>
   JJ: Lines beginning with '-' are deletions and cannot be edited.
   JJ: Edit '+' (added) and ' ' (context) lines; save to apply.
   <file path>
   @@ -<old_start>,<old_len> +<new_start>,<new_len> @@
    context line
   -deleted line
   +added line
    context line
   ```

4. Edit the `+` (additions) and ` ` (context) lines. `-` (deletion)
   lines are read-only — editing them has no effect.
5. Save. Bad Juju:
   - Recomputes the `@@` header (the line lengths after your edits).
   - Runs `jj squash --interactive --tool badjuju` to apply the edited
     hunk to the source commit.
   - Refreshes the squash window so you can continue picking hunks.

## Status messages

The buffer's first action after save is to print a terminal status
line:

| Status | Meaning |
| ------ | ------- |
| `EDIT APPLIED` | The hunk was applied successfully. |
| `EDIT ABORTED` | The body was cleared; no change was made. |
| `STALE SOURCE` | The source commit was abandoned (or otherwise rewritten) while you were editing. Reopen the squash window and try again. |

## Constraints

- Only **one hunk-edit buffer at a time.** The path is
  `.jj/badjuju/hunk-edit.jujutsu` — a single shared location. If you
  open a hunk-edit for a second hunk, it replaces the first.
- The buffer is only meaningful when the parent squash window is
  still alive. Closing the squash window invalidates the edit.
- `JJ:`-prefixed lines and the `@@` header are advisory metadata.
  Don't edit them — Bad Juju regenerates them on save based on your
  edits to the body.

## Key bindings

| Client | Finalize (save) | Close without saving |
| ------ | --------------- | -------------------- |
| Neovim | `:write` | `:bd!` |
| VS Code | `Ctrl+S` | Close editor without save |
| Emacs | `C-x C-s` | `C-c C-k` (where bound) |

The buffer behaves like a normal editor file — save semantics drive
the apply step, not a dedicated keybinding.
