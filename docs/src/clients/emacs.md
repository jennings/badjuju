# Emacs

The Emacs client lives in `clients/emacs/` and is modeled on Magit.
It uses `eglot` (Emacs 29+ built-in LSP client) and `transient` (the
popup-menu library Magit itself uses).

## Requirements

- Emacs 29+ (ships built-in `eglot` and `transient`)
- `badjuju` server binary on your `PATH`
- `jj` on your `PATH`

## Installation

### Vanilla Emacs (manual)

```emacs-lisp
(add-to-list 'load-path "/path/to/badjuju/clients/emacs")
(require 'badjuju)
```

### `use-package`

```emacs-lisp
(use-package badjuju
  :load-path "/path/to/badjuju/clients/emacs"
  :commands (badjuju-status badjuju-log badjuju-diff))
```

### `straight.el`

```emacs-lisp
(straight-use-package
 '(badjuju :type git
            :host github
            :repo "jennings/badjuju"
            :files ("clients/emacs/*.el")))
```

Or with `use-package` integration:

```emacs-lisp
(use-package badjuju
  :straight (badjuju :type git
                      :host github
                      :repo "jennings/badjuju"
                      :files ("clients/emacs/*.el"))
  :commands (badjuju-status badjuju-log badjuju-diff))
```

### Doom Emacs

In `packages.el`:

```emacs-lisp
(package! badjuju
  :recipe (:host github :repo "jennings/badjuju"
           :files ("clients/emacs/*.el")))
```

In `config.el`:

```emacs-lisp
(use-package! badjuju
  :commands (badjuju-status badjuju-log badjuju-diff)
  :config
  (map! :leader "g j" #'badjuju-status))
```

## Configuration

```emacs-lisp
;; Path to the jj binary; nil uses PATH.
(setq badjuju-binary-path nil)

;; Hotkey profile: "magit" (default) or "none".
(setq badjuju-keymap-profile "magit")
```

## Top-level commands

| Command | Description |
| ------- | ----------- |
| `M-x badjuju-status` | Open the working-copy status view |
| `M-x badjuju-log` | Open the commit log |
| `M-x badjuju-diff` | Diff for `@` (change mode, updates on amend) |
| `M-x badjuju-describe` | Edit the commit message for `@` |
| `M-x badjuju-new` | Create a new child change |
| `M-x badjuju-edit` | Move `@` to a different commit |
| `M-x badjuju-abandon` | Abandon the working copy |
| `M-x badjuju-squash` | Squash into the parent |
| `M-x badjuju-unsquash` | Pull a file back from the parent |
| `M-x badjuju-undo` | Undo the last operation |
| `M-x badjuju-fetch` | `jj git fetch` |
| `M-x badjuju-push` | `jj git push` |
| `M-x badjuju-refresh` | Refresh the current buffer |

Most workflows live in the status buffer — open it with
`M-x badjuju-status` and use the hotkeys below. Press `?` at any time
for a popup of active bindings in the current buffer.

## Keybindings (magit profile)

### Status buffer

| Key | Action |
| --- | ------ |
| `n` | New child change |
| `c` | Commit transient (reword / new child) |
| `c w` | Commit transient → reword (describe) commit at cursor |
| `c n` | Commit transient → new child commit |
| `d` | Diff change at cursor (updates on amend) |
| `D` | Diff commit at cursor (pinned, immutable) |
| `=` | Diff (alias for `d`) |
| `e` | Edit commit at cursor (move `@`) |
| `a` | Abandon commit at cursor |
| `s` | Select squash source or destination (two-step) |
| `S` | Squash file at cursor into parent |
| `u` | Unsquash file at cursor |
| `U` | Undo last operation |
| `r s` | Mark rebase source (`--source`) |
| `r r` | Mark rebase source (`--revisions`) |
| `r b` | Mark rebase source (`--branch`) |
| `r o` | Execute rebase onto cursor |
| `r A` | Execute rebase insert-after cursor |
| `r B` | Execute rebase insert-before cursor |
| `x` | Cancel pending squash or rebase |
| `b` | Bookmark manager |
| `f` | `jj git fetch` |
| `p` | `jj git push` |
| `P` | `jj git push --force-with-lease` |
| `L` | Open log |
| `R` | Refresh |
| `TAB` | Toggle fold at cursor |
| `RET` | Go to definition |
| `gd` | Go to definition |
| `?` | Show help popup |
| `q` | Bury buffer |

Code actions intentionally have no default binding — use the global
`M-x eglot-code-actions` (Emacs binds it to `C-c C-a` by default in
eglot-managed buffers).

### Log buffer

| Key | Action |
| --- | ------ |
| `c` | Commit transient (reword / new child) |
| `c w` | Commit transient → reword (describe) commit at cursor |
| `c n` | Commit transient → new child commit |
| `d` | Diff change at cursor (updates on amend) |
| `D` | Diff commit at cursor (pinned, immutable) |
| `=` | Diff (alias for `d`) |
| `e` | Edit commit at cursor |
| `a` | Abandon commit at cursor |
| `s` | Select squash source or destination (two-step) |
| `S` | Squash file at cursor into parent |
| `U` | Undo |
| `r s` | Mark rebase source (`--source`) |
| `r r` | Mark rebase source (`--revisions`) |
| `r b` | Mark rebase source (`--branch`) |
| `r o` | Execute rebase onto cursor |
| `r A` | Execute rebase insert-after cursor |
| `r B` | Execute rebase insert-before cursor |
| `x` | Cancel pending squash or rebase |
| `b` | Bookmark manager |
| `R` | Refresh |
| `RET` | Apply revset shortcut on a `JJ:` line / go to definition |
| `gd` | Go to definition |
| `?` | Show help popup |
| `q` | Bury buffer |

### Diff buffer

| Key | Action |
| --- | ------ |
| `R` | Refresh |
| `RET` / `gd` | Go to definition |
| `?` | Show help popup |
| `q` | Bury buffer |

### Squash buffer

| Key | Action |
| --- | ------ |
| `s` | Toggle hunk/file between SELECTED and REMAINING |
| `e` | Edit hunk before squashing |
| `a` | Select all changes |
| `A` | Deselect all changes |
| `u` | Undo |
| `TAB` | Toggle fold |
| `gd` | Go to definition |
| `?` | Show help popup |
| `q` | Bury buffer |

### Describe buffer

| Key | Action |
| --- | ------ |
| `C-c C-c` | Finalize and close (saves the commit message) |
| `C-c C-k` | Abort without saving |

## Folding

Status and squash buffers open fully folded. The `WORKING COPY
CHANGES` and `PARENT CHANGES` sections are automatically expanded on
first open; `TAB` toggles individual sections.
