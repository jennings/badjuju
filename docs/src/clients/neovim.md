# Neovim

The Neovim client lives in `clients/neovim/` in the repo. It uses
Neovim 0.11's built-in LSP API (`vim.lsp.enable`) with automatic
workspace detection (`root_markers = { '.jj' }`).

## Requirements

- Neovim 0.11+
- `badjuju` server binary on your `PATH` (see [Getting
  Started](../getting-started.md#1-install-the-server))

## Installation

Pick the recipe that matches your plugin manager. Replace
`/absolute/path/to/bad-juju` with the path to your local checkout.

### lazy.nvim (recommended)

```lua
{
  dir = '/absolute/path/to/bad-juju/clients/neovim',
  name = 'bad-juju',
  ft = 'jujutsu',
  opts = {},
}
```

### packer.nvim

```lua
use {
  '/absolute/path/to/bad-juju/clients/neovim',
  config = function() require('badjuju').setup({}) end,
}
```

### vim-plug

```vim
Plug 'jennings/bad-juju', { 'rtp': 'clients/neovim' }
```

```lua
require('badjuju').setup({})
```

### pathogen

```sh
ln -s /absolute/path/to/bad-juju/clients/neovim \
  ~/.vim/bundle/bad-juju
```

### Vundle

```vim
Plugin 'jennings/bad-juju'
set rtp+=~/.vim/bundle/bad-juju/clients/neovim
```

### Neovim built-in packages

```sh
ln -s /absolute/path/to/bad-juju/clients/neovim \
  ~/.local/share/nvim/site/pack/badjuju/start/bad-juju
```

### Manual / no plugin manager

```lua
vim.opt.rtp:prepend('/absolute/path/to/bad-juju/clients/neovim')
require('badjuju').setup({})
```

## Configuration

`setup()` is optional. Only call it if you want to override defaults:

```lua
require('badjuju').setup({
  -- Path to the jj binary; forwarded to the server.
  binaryPath = nil,
  -- Default revset for :JJLog when called with no argument.
  defaultLogRevset = nil,
  -- Hotkey profile: "magit" (default), "vim", or "none".
  keymapProfile = nil,
})
```

## Commands

| Command | Description |
| ------- | ----------- |
| `:JJStatus` | Open `.jj/badjuju/status.jujutsu` |
| `:JJLog [revset]` | Open the log; defaults to `@` |
| `:JJDescribe [revision]` | Edit a commit message (defaults to `@`) |
| `:JJDiff [revision]` | Open a diff (defaults to `@`) |
| `:JJNew` | Create a new change |
| `:JJRefresh` | Refresh the badjuju buffer at the cursor |
| `:JJSquash [file] [revision]` | Squash a file into its parent |
| `:JJUnsquash [file] [revision]` | Unsquash a file from parent into child |
| `:JJUndo` | Run `jj undo` and refresh |
| `:JJAbandon [revision]` | Abandon a revision (defaults to `@`) |

Commands auto-start the LSP for the current workspace if it isn't
already running.

## Keymaps

Two built-in profiles, plus `"none"` to disable defaults. Set
`keymapProfile` in `setup()` to switch.

### `magit` profile (default)

#### `status.jujutsu`

| Key      | Action |
| -------- | ------ |
| `R`      | Refresh |
| `n`      | New change |
| `L`      | Open log |
| `f`      | Git fetch |
| `p`      | Git push |
| `P`      | Git push --force-with-lease |
| `e`      | Edit commit at cursor (move @) |
| `b`      | Bookmark manager |
| `r`      | Rebase commit at cursor |
| `d`      | Diff change at cursor (updates on amend) |
| `D`      | Diff commit at cursor (pinned, immutable) |
| `c w`    | Describe commit at cursor |
| `s`      | Squash file at cursor into parent |
| `u`      | Unsquash file at cursor |
| `a`      | Abandon commit at cursor |
| `U`      | Undo |
| `=`      | Diff (alias for `d`) |
| `q`      | Close window |
| `?`      | Show key binding help |

#### `log.jujutsu`

| Key      | Action |
| -------- | ------ |
| `R`      | Refresh |
| `e`      | Edit commit at cursor (move @) |
| `b`      | Bookmark manager |
| `r`      | Rebase commit at cursor |
| `d`      | Diff change at cursor (updates on amend) |
| `D`      | Diff commit at cursor (pinned, immutable) |
| `c w`    | Describe commit at cursor |
| `a`      | Abandon commit at cursor |
| `U`      | Undo |
| `=`      | Diff (alias for `d`) |
| `<CR>`   | Apply revset shortcut on cursor line |
| `q`      | Close window |
| `?`      | Show key binding help |

#### `diff.jujutsu`

| Key      | Action |
| -------- | ------ |
| `R`      | Refresh (re-runs `jj diff`) |
| `q`      | Close window |
| `?`      | Show key binding help |

#### `describe.jujutsu`

| Key (mode)            | Action |
| --------------------- | ------ |
| `<C-c><C-c>` (normal) | Finalize commit (save and close) |
| `<C-c><C-k>` (normal) | Abort (close without saving) |
| `<C-c>` (insert)      | Finalize commit (save and close) |
| `?` (normal)          | Show key binding help |

### `vim` profile

Two-letter verb chords inspired by Fugitive. Most actions use doubled
letters (`nn`, `dd`, etc.) to keep single keys free for text
navigation:

| Key | Action |
| --- | ------ |
| `nn` | New change |
| `ll` | Open log |
| `dd` | Describe commit at cursor |
| `D`  | Diff commit at cursor |
| `ss` | Squash file at cursor |
| `uu` | Undo |
| `UU` | Unsquash file at cursor |
| `aa` | Abandon commit at cursor |
| … | (`ee`, `bb`, `rr`, `ff`, `pp`, `PP` follow the same convention) |

Enable with:

```lua
require('badjuju').setup({ keymapProfile = 'vim' })
```

### `none` profile

Disables all built-in keymaps. Define your own using the `:JJ*`
commands above.

## Auto-refresh

Open status, log, and diff buffers auto-refresh whenever a `jj`
operation runs — even one you ran in a terminal. No manual reload
required.

## Syntax highlighting

Highlights come from the LSP via semantic tokens. Your colorscheme's
standard token groups (comments, keywords, strings, types, enum
members, numbers, operators) are picked up automatically; no extra
configuration needed.
