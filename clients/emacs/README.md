# Bad Juju — Emacs Client

Emacs frontend for the Jujutsu VCS, modeled on Magit.  Powered by
`eglot` (Emacs 29+ built-in LSP client) and `transient` (same library
Magit uses for its popup menus).

## Requirements

- Emacs 29+ (ships built-in `eglot` and `transient`)
- `badjuju` server binary on your `PATH`
- `jj` on your `PATH`

### Install the server binary

From a checkout of the badjuju repo:

```sh
redo server/install   # installs to ~/.cargo/bin/badjuju
```

> No `redo` installed? Run `./do server/install` instead — the repo
> ships a self-contained `./do` shell-script fallback.

Make sure `~/.cargo/bin` is on your `PATH`.

---

## Installation

### Vanilla Emacs (manual)

1. Clone the repository or download `clients/emacs/`.
2. Add the directory to your `load-path` and require the package:

```emacs-lisp
(add-to-list 'load-path "/path/to/badjuju/clients/emacs")
(require 'badjuju)
```

### `use-package` (built-in, Emacs 29+)

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

Add to `packages.el`:

```emacs-lisp
(package! badjuju
  :recipe (:host github :repo "jennings/badjuju"
           :files ("clients/emacs/*.el")))
```

Add to `config.el`:

```emacs-lisp
(use-package! badjuju
  :commands (badjuju-status badjuju-log badjuju-diff)
  :config
  ;; Optional: bind a global key to the status buffer
  (map! :leader "g j" #'badjuju-status))
```

---

## Configuration

### `badjuju-binary-path`

Path to the `jj` binary. Leave blank (the default) to use `jj` on your
`PATH`.

```emacs-lisp
(setq badjuju-binary-path "/usr/local/bin/jj")
```

### `badjuju-keymap-profile`

Controls the hotkey style in Bad Juju buffers.

| Value | Description |
|-------|-------------|
| `"magit"` | Single-key bindings following Magit conventions (default) |
| `"none"` | No default bindings — define your own |

```emacs-lisp
(setq badjuju-keymap-profile "magit")
```

---

## Usage

### Open the status buffer

```
M-x badjuju-status
```

All other commands are available from the status buffer via hotkeys.

### Top-level commands

| Command | Description |
|---------|-------------|
| `M-x badjuju-status` | Open the working-copy status view |
| `M-x badjuju-log` | Open the commit log |
| `M-x badjuju-diff` | Open a change diff for `@` (updates on amend) |
| `M-x badjuju-describe` | Edit the commit message for `@` |
| `M-x badjuju-new` | Create a new child change |
| `M-x badjuju-edit` | Move `@` to a different commit |
| `M-x badjuju-abandon` | Abandon the working copy commit |
| `M-x badjuju-squash` | Squash into the parent |
| `M-x badjuju-unsquash` | Pull a file back from the parent |
| `M-x badjuju-undo` | Undo the last operation |
| `M-x badjuju-fetch` | `jj git fetch` |
| `M-x badjuju-push` | `jj git push` |
| `M-x badjuju-refresh` | Refresh the current buffer |

---

## Keybindings (magit profile)

Press `?` in any Bad Juju buffer for an in-editor popup listing all
active bindings for that buffer type.

### Status buffer

| Key | Action |
|-----|--------|
| `n` | New child change |
| `c w` | Reword (describe) commit at cursor |
| `c n` | New child of commit at cursor |
| `d` | Describe commit at cursor |
| `D` | Diff commit at cursor (change mode, updates on amend) |
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
| `b` | Bookmark (create / move / delete / track / forget) |
| `f` | `jj git fetch` |
| `p` | `jj git push` |
| `P` | `jj git push --force-with-lease` |
| `L` | Open log |
| `R` | Refresh |
| `TAB` | Toggle fold at cursor |
| `RET` | Go to definition |
| `gd` | Go to definition |
| `A` / `M-RET` | Code actions at cursor |
| `?` | Show help popup |
| `q` | Bury buffer |

### Log buffer

| Key | Action |
|-----|--------|
| `c w` | Reword commit at cursor |
| `c n` | New child of commit at cursor |
| `d` | Describe commit at cursor |
| `D` | Diff commit at cursor |
| `e` | Edit commit at cursor (move `@`) |
| `a` | Abandon commit at cursor |
| `s` | Select squash source or destination (two-step) |
| `S` | Squash file at cursor into parent |
| `U` | Undo last operation |
| `r s` | Mark rebase source (`--source`) |
| `r r` | Mark rebase source (`--revisions`) |
| `r b` | Mark rebase source (`--branch`) |
| `r o` | Execute rebase onto cursor |
| `r A` | Execute rebase insert-after cursor |
| `r B` | Execute rebase insert-before cursor |
| `x` | Cancel pending squash or rebase |
| `b` | Bookmark management |
| `R` | Refresh |
| `RET` | Apply revset shortcut (on `JJ:` line) / go to definition |
| `gd` | Go to definition |
| `A` / `M-RET` | Code actions |
| `?` | Show help popup |
| `q` | Bury buffer |

### Diff buffer

| Key | Action |
|-----|--------|
| `R` | Refresh |
| `RET` / `gd` | Go to definition |
| `A` / `M-RET` | Code actions |
| `?` | Show help popup |
| `q` | Bury buffer |

### Squash buffer

| Key | Action |
|-----|--------|
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
|-----|--------|
| `C-c C-c` | Finalize and close (saves the commit message) |
| `C-c C-k` | Abort without saving |

---

## Log shortcut lines

In the log buffer, lines beginning with `JJ:` are named revset shortcuts
pre-populated by the server (e.g. `JJ: @ :  @`).  Place the cursor on a
`JJ:` line and press `RET` to re-run the log with that revset.  You can
also edit the `REVSET:` header at the top of the buffer and save the file
to re-run the query with a custom revset.

---

## Folding

Status and squash buffers open fully folded.  `WORKING COPY CHANGES` and
`PARENT CHANGES` sections are automatically expanded on first open.  Press
`TAB` to toggle individual sections.
