# Kakoune

The Kakoune client lives in `clients/kakoune/` in the repo. It uses
[kak-lsp](https://github.com/kakoune-lsp/kakoune-lsp) to talk to the
`badjuju` LSP server and exposes all operations as `:JJ*` commands with
an optional keymap via Kakoune user-modes.

## Requirements

- Kakoune 2023.08.05+
- kak-lsp 0.14+ on your `PATH`
- `jj` on your `PATH`
- `badjuju` binary on your `PATH` (see [Getting
  Started](../getting-started.md#1-install-the-server))

## Install

### plug.kak

```kak
plug "jennings/bad-juju" subset [clients/kakoune/badjuju.kak] config %{
    # Optional: switch to vim-profile chords (default: magit)
    # set-option global badjuju_keymap_profile vim
}
```

### Manual clone

Symlink the entry point into your autoload directory:

```sh
ln -s /absolute/path/to/bad-juju/clients/kakoune/badjuju.kak \
      ~/.config/kak/autoload/badjuju.kak
```

Or source it directly from your `kakrc`:

```kak
source '/absolute/path/to/bad-juju/clients/kakoune/badjuju.kak'
```

## Setup

### 1. Configure kak-lsp

Merge the `kak-lsp.toml` snippet from this directory into
`~/.config/kak-lsp/kak-lsp.toml`:

```sh
cat /path/to/bad-juju/clients/kakoune/kak-lsp.toml \
    >> ~/.config/kak-lsp/kak-lsp.toml
```

Then start kak-lsp in your `kakrc` (if not already):

```kak
eval %sh{ kak-lsp --kakoune -s $kak_session }
```

### 2. Choose a keymap profile

The default is `magit` (single-letter bindings). To switch to the
vim profile (double-letter chords), set the option **before** sourcing
the plugin:

```kak
set-option global badjuju_keymap_profile vim
source '/path/to/badjuju.kak'
```

Or in plug.kak config:

```kak
plug "jennings/bad-juju" ... config %{
    set-option global badjuju_keymap_profile vim
}
```

## Opening buffers

The canonical one-liner opens the working-copy status view in Kakoune:

```sh
kak "$(badjuju status)"
```

Similarly for other views:

```sh
kak "$(badjuju log)"
kak "$(badjuju diff)"
kak "$(badjuju diff --revision abc123)"
```

## Commands

| Command | Description |
| ------- | ----------- |
| `:JJStatus` | Open `.jj/badjuju/status.jujutsu` |
| `:JJLog [revset]` | Open the log buffer |
| `:JJLogFile` | Open per-file log for the file at cursor |
| `:JJDescribe [revision]` | Edit a commit message (default: @) |
| `:JJDiff [revision]` | Open a change diff (updates on amend) |
| `:JJDiffCommit [revision]` | Open a pinned commit diff |
| `:JJNew` | Create a new change |
| `:JJNext` | Move @ to the next child |
| `:JJPrev` | Move @ to the previous parent |
| `:JJRefresh` | Refresh the current badjuju buffer |
| `:JJSquash` | Squash file at cursor into its parent |
| `:JJUnsquash` | Unsquash file at cursor from parent |
| `:JJUndo` | Undo the last jj operation |
| `:JJAbandon [revision]` | Abandon a revision (default: @ at cursor) |
| `:JJEdit [revision]` | Move @ to this revision |
| `:JJFetch` | Run `jj git fetch` |
| `:JJPush [!]` | Run `jj git push` (`!` for `--force-with-lease`) |
| `:JJCancel` | Cancel pending squash or rebase |

Commands auto-start the LSP for the current workspace if it isn't already
running.

## Keymap reference

With a `*.jujutsu` buffer active, press `<space>` to enter the `badjuju`
user-mode. The bindings below use the **magit** profile (default).

### `magit` profile — `status.jujutsu`

| Key | Action |
| --- | ------ |
| `R` | Refresh |
| `n` | New change |
| `L` | Open log |
| `f` | Git fetch |
| `p` | Git push |
| `P` | Git push --force-with-lease |
| `U` | Undo |
| `a` | Abandon revision at cursor |
| `e` | Edit commit at cursor (move @) |
| `d` | Diff change at cursor (updates on amend) |
| `D` | Diff commit at cursor (pinned) |
| `s` | Select squash source/dest (two-step) |
| `S` | Squash file at cursor into parent |
| `u` | Unsquash file at cursor |
| `x` | Cancel pending operation |
| `q` | Close buffer |
| `b` | Bookmark… (chord) |
| `r` | Rebase… (chord) |
| `c` | Commit… (chord) |
| `?` | Show key binding help |

### `magit` profile — `log.jujutsu`

| Key | Action |
| --- | ------ |
| `R` | Refresh |
| `n` | New change |
| `U` | Undo |
| `a` | Abandon revision at cursor |
| `e` | Edit commit at cursor (move @) |
| `d` | Diff change at cursor (updates on amend) |
| `D` | Diff commit at cursor (pinned) |
| `s` | Select squash source/dest (two-step) |
| `x` | Cancel pending operation |
| `q` | Close buffer |
| `b` | Bookmark… (chord) |
| `r` | Rebase… (chord) |
| `c` | Commit… (chord) |
| `?` | Show key binding help |

### `magit` profile — `diff.jujutsu`

| Key | Action |
| --- | ------ |
| `R` | Refresh |
| `q` | Close buffer |
| `?` | Show key binding help |

### `magit` profile — squash window

| Key | Action |
| --- | ------ |
| `s` | Toggle hunk/file at cursor |
| `e` | Edit hunk before squashing |
| `a` | Select all changes |
| `A` | Deselect all changes |
| `q` | Close buffer |
| `?` | Show key binding help |

### `magit` profile — `describe.jujutsu`

| Key | Action |
| --- | ------ |
| `<C-c><C-c>` | Save and close (finalize commit message) |
| `<C-c><C-k>` | Abort (close without saving) |
| `?` | Show key binding help |

## Chord workflows

### Bookmark (`b` prefix in magit / `bb` prefix in vim)

| Key | Action |
| --- | ------ |
| `c` | Create bookmark (prompts for name) |
| `m` | Move bookmark (prompts for name) |
| `d` | Delete bookmark (prompts for name) |
| `t` | Track remote bookmark (prompts for `name@remote`) |
| `f` | Forget bookmark (prompts for name) |

### Rebase (`r` / `rr`)

| Key | Action |
| --- | ------ |
| `s` | Mark source with `--source` |
| `r` | Mark source with `--revisions` |
| `b` | Mark source with `--branch` |
| `o` | Complete: rebase onto this commit |
| `A` | Complete: insert after this commit |
| `B` | Complete: insert before this commit |

### Commit transient (`c` / `cc`)

| Key | Action |
| --- | ------ |
| `w` | Reword — open `describe.jujutsu` |
| `n` | New child commit |

### Commit-to-commit squash

1. Press `s` on the **source** commit in status or log → server marks it.
2. Press `s` on the **destination** commit → server opens the squash window.
3. In the squash window, use `s`/`e`/`a`/`A` to manage hunks.
4. `:w` finalizes the squash; `:q!` aborts.

## `vim` profile

Double-letter chords inspired by Fugitive. Enable with:

```kak
set-option global badjuju_keymap_profile vim
```

| Key | Action |
| --- | ------ |
| `nn` | New change |
| `ll` | Open log |
| `ee` | Edit commit at cursor (move @) |
| `dd` | Describe commit |
| `dd` | Diff change at cursor (updates on amend) |
| `DD` | Diff commit at cursor (pinned) |
| `ss` | Squash source/dest (two-step) |
| `SS` | Squash file at cursor |
| `uu` | Unsquash file at cursor |
| `UU` | Undo |
| `aa` | Abandon revision |
| `ff` | Git fetch |
| `pp` | Git push |
| `PP` | Git push --force-with-lease |
| `RR` | Refresh |
| `qq` | Close buffer |
| `x` / `xx` | Cancel pending operation |
| `bb` + c/m/d/t/f | Bookmark chord |
| `rr` + s/r/b/o/A/B | Rebase chord |
| `cc` + w/n | Commit transient chord |

## Save flow

- `describe.jujutsu`: edit the commit message, then `:w` runs `jj describe -m <message>`.
  `<C-c><C-c>` saves and closes; `<C-c><C-k>` aborts (magit profile).
- `hunk-edit-*.jujutsu`: move hunks between SELECTED/REMAINING sections,
  then `:w` applies the selection via `jj squash`.

## Customization

### Override the `<space>` leader

Re-bind `<space>` after sourcing the plugin:

```kak
hook global WinSetOption filetype=jujutsu %{
    map window normal <tab> ': enter-user-mode badjuju-status<ret>'
}
```

### Disable the default keymap

Set the profile to any unrecognized value; the source block in `badjuju.kak`
will then source `keymap-magit.kak` (fallback). To truly disable, edit the
sourcing logic or just don't map `<space>`:

```kak
hook global WinSetOption filetype=jujutsu %{
    unmap window normal <space>
}
```

## Troubleshooting

**kak-lsp not attaching to jujutsu buffers**

Check that the `[language.jujutsu]` section is present in
`~/.config/kak-lsp/kak-lsp.toml` and that kak-lsp is running:

```kak
:lsp-show-server
```

**`:w` on `describe.jujutsu` has no effect**

Make sure `include_text_on_save = true` is set in your `kak-lsp.toml`
snippet. Without it, the server receives the save event but no text.

**Auto-reload not firing after mutations**

`workspace/applyEdit` is the mechanism the server uses to push updated
content to open buffers. Verify kak-lsp is attached (`lsp-show-server`)
and that it supports apply-edit (kak-lsp 0.14+).
