# Bad Juju — Neovim Client

Neovim LSP configuration and plugin for the Bad Juju Jujutsu VCS integration.

## Requirements

- Neovim 0.11+
- `badjuju` binary on your `PATH` (or specify the full path in `cmd`)

## Setup

### 1. Install from source

There's no published plugin yet, so you install by cloning the bad-juju
repository and pointing your plugin manager (or runtimepath) at
`clients/neovim/`. The same directory provides:

- `ftdetect/jujutsu.lua` — registers the `jujutsu` filetype for `*.jujutsu`
- `plugin/badjuju.lua` — registers the `:JJ*` user commands on startup
- `lsp/jujutsu.lua` — LSP server config consumed by `vim.lsp.enable`

You also need the `badjuju` server binary on your `PATH`. From a checkout
of the bad-juju repo:

```sh
redo server/install   # installs to ~/.cargo/bin/badjuju
```

#### Option A — lazy.nvim (recommended)

```lua
{
  dir = '/absolute/path/to/bad-juju/clients/neovim',
  name = 'bad-juju',
  ft = 'jujutsu',
  config = function()
    require('badjuju').setup({})
    vim.lsp.enable('jujutsu')
  end,
}
```

`dir = ...` makes lazy.nvim load the on-disk checkout directly — no clone
step. Replace the path with wherever you cloned bad-juju. `ft = 'jujutsu'`
defers loading until you open a `*.jujutsu` buffer (filetype detection still
happens eagerly because `ftdetect/` is sourced at startup).

#### Option B — packer.nvim

```lua
use {
  '/absolute/path/to/bad-juju/clients/neovim',
  config = function()
    require('badjuju').setup({})
    vim.lsp.enable('jujutsu')
  end,
}
```

#### Option C — vim-plug

The plugin lives in the `clients/neovim/` subdirectory of the repo, so
scope the runtimepath with vim-plug's `rtp` option:

```vim
Plug 'jennings/bad-juju', { 'rtp': 'clients/neovim' }
```

```lua
require('badjuju').setup({})
vim.lsp.enable('jujutsu')
```

#### Option D — pathogen

Pathogen has no subdirectory option, so symlink `clients/neovim/` into
your bundle directory:

```sh
ln -s /absolute/path/to/bad-juju/clients/neovim ~/.vim/bundle/bad-juju
```

```lua
require('badjuju').setup({})
vim.lsp.enable('jujutsu')
```

#### Option E — Vundle

Vundle has no subdirectory option, so add the subpath to `runtimepath`
after `vundle#end()`:

```vim
Plugin 'jennings/bad-juju'
" Plugin lives in clients/neovim/.
set rtp+=~/.vim/bundle/bad-juju/clients/neovim
```

```lua
require('badjuju').setup({})
vim.lsp.enable('jujutsu')
```

#### Option F — Neovim built-in packages (`pack/*/start/`)

Symlink the `clients/neovim/` subdirectory into a start path:

```sh
ln -s /absolute/path/to/bad-juju/clients/neovim \
  ~/.local/share/nvim/site/pack/badjuju/start/bad-juju
```

```lua
require('badjuju').setup({})
vim.lsp.enable('jujutsu')
```

#### Option G — manual / no plugin manager

Add the directory to `runtimepath` and call `setup` + `vim.lsp.enable`
yourself:

```lua
vim.opt.rtp:prepend('/absolute/path/to/bad-juju/clients/neovim')
require('badjuju').setup({})
vim.lsp.enable('jujutsu')
```

### 2. Configure (optional) and enable the LSP

```lua
require('badjuju').setup({
  -- Path to the jj binary; forwarded to the server as init_options.binaryPath.
  -- Leave nil to use jj from PATH.
  binaryPath = nil,
  -- Default revset used by :JJLog when called with no argument.
  -- Leave nil to match the status window's stack:
  -- ancestors(reachable(@, mutable()), 2).
  defaultLogRevset = nil,
})
vim.lsp.enable('jujutsu')
```

`setup()` is optional — without it, the plugin behaves as if both values are
unset. It must be called *before* `vim.lsp.enable('jujutsu')` because the LSP
config table reads `require('badjuju').config` at enable time.

The LSP config uses Neovim 0.11's built-in `vim.lsp.enable` API with
`root_markers = { '.jj' }` for automatic workspace detection. The server
attaches to buffers with filetype `jujutsu`.

## Commands

All commands send a `workspace/executeCommand` request to the running
`jujutsu` LSP and open the returned file in the current window.

| Command                         | Description                                                      |
| ------------------------------- | ---------------------------------------------------------------- |
| `:JJStatus`                     | Open `.jj/badjuju/status.jujutsu`                                |
| `:JJLog [revset]`               | Open `.jj/badjuju/log.jujutsu`; defaults to `@`                  |
| `:JJDescribe [revision]`        | Open the describe buffer for a revision (defaults to `@`)        |
| `:JJDiff [revision]`            | Open `.jj/badjuju/diff.jujutsu` for a revision (defaults to `@`) |
| `:JJNew`                        | Create a new change and refresh log                              |
| `:JJRefresh`                    | Refresh the badjuju buffer at the cursor                         |
| `:JJSquash [file] [revision]`   | Squash a file into its parent (see `s` keymap below)             |
| `:JJUnsquash [file] [revision]` | Unsquash a file from parent into child (see `U` keymap below)    |
| `:JJToggleStat`                 | Toggle `--stat` rendering in log                                 |
| `:JJUndo`                       | Run `jj undo` and refresh                                        |
| `:JJAbandon [revision]`         | Abandon a revision (defaults to `@`)                             |

Commands auto-start the `jujutsu` LSP for the current workspace if it isn't
already running, so `:JJStatus` works from any buffer inside a jj workspace
(no need to open a `.jujutsu` file first). Outside a jj workspace, the command
reports an error.

## Keymaps

The plugin installs buffer-local normal-mode maps on the generated
`status.jujutsu`, `log.jujutsu`, `diff.jujutsu`, and `describe.jujutsu`
buffers. The active profile is set via `keymapProfile` in `setup()`.

Two built-in profiles are available. The **default (magit-style) profile is
active whenever `keymapProfile` is unset, `nil`, or `"magit"`**.

### Default profile — single-key bindings

Inspired by Magit/Lazygit conventions.

#### `status.jujutsu`

| Key      | Action                                             |
| -------- | -------------------------------------------------- |
| `R`      | Refresh                                            |
| `n`      | New change (`:JJNew`)                              |
| `L`      | Open log (`:JJLog`)                                |
| `f`      | Git fetch                                          |
| `p`      | Git push                                           |
| `P`      | Git push --force-with-lease                        |
| `e`      | Edit commit at cursor (move @)                     |
| `b`      | Bookmark (create / move / delete / track / forget) |
| `r`      | Rebase commit at cursor to destination             |
| `d`      | Describe commit at cursor in a split               |
| `D`      | Diff commit at cursor in a split                   |
| `s`      | Squash file at cursor into parent                  |
| `U`      | Unsquash file at cursor from parent into child     |
| `a`      | Abandon commit at cursor                           |
| `u`      | Undo (`:JJUndo`)                                   |
| `=`      | Toggle `--stat` rendering in STACK section         |
| `q`      | Close window                                       |
| `?`      | Show key binding help                              |

#### `log.jujutsu`

| Key      | Action                                                 |
| -------- | ------------------------------------------------------ |
| `R`      | Refresh                                                |
| `e`      | Edit commit at cursor (move @)                         |
| `b`      | Bookmark                                               |
| `r`      | Rebase commit at cursor to destination                 |
| `d`      | Describe commit at cursor in a split                   |
| `D`      | Diff commit at cursor in a split                       |
| `a`      | Abandon commit at cursor                               |
| `<CR>`   | Apply revset shortcut on cursor line (no-op elsewhere) |
| `q`      | Close window                                           |
| `?`      | Show key binding help                                  |
#### `diff.jujutsu`

| Key      | Action                                            |
| -------- | ------------------------------------------------- |
| `R`      | Refresh (re-runs `jj diff` for the same revision) |
| `q`      | Close window                                      |
| `?`      | Show key binding help                             |

#### `describe.jujutsu`

| Key (mode)            | Action                           |
| --------------------- | -------------------------------- |
| `<C-c><C-c>` (normal) | Finalize commit (save and close) |
| `<C-c><C-k>` (normal) | Abort (close without saving)     |
| `<C-c>` (insert)      | Finalize commit (save and close) |
| `?` (normal)          | Show key binding help            |

---

### `vim` profile — two-letter verb chords

Enable with `keymapProfile = "vim"` in `setup()`. Inspired by Fugitive-style
bindings; most actions use doubled letters (`nn`, `dd`, etc.) so single keys
remain available for text navigation. A few unambiguous actions keep single
keys.

```lua
require('badjuju').setup({ keymapProfile = 'vim' })
```

#### `status.jujutsu`

| Key  | Action                                         |
| ---- | ---------------------------------------------- |
| `nn` | New change                                     |
| `ll` | Open log                                       |
| `ff` | Git fetch                                      |
| `pp` | Git push                                       |
| `PP` | Git push --force-with-lease                    |
| `ee` | Edit commit at cursor (move @)                 |
| `bb` | Bookmark                                       |
| `rr` | Rebase commit at cursor to destination         |
| `dd` | Describe commit at cursor in a split           |
| `D`  | Diff commit at cursor in a split               |
| `ss` | Squash file at cursor into parent              |
| `uu` | Undo                                           |
| `UU` | Unsquash file at cursor from parent into child |
| `aa` | Abandon commit at cursor                       |
| `=`  | Toggle `--stat` rendering in STACK section     |
| `q`  | Close window                                   |
| `?`  | Show key binding help                          |

#### `log.jujutsu`

| Key      | Action                                 |
| -------- | -------------------------------------- |
| `R`      | Refresh                                |
| `ee`     | Edit commit at cursor (move @)         |
| `bb`     | Bookmark                               |
| `rr`     | Rebase commit at cursor to destination |
| `dd`     | Describe commit at cursor in a split   |
| `D`      | Diff commit at cursor in a split       |
| `aa`     | Abandon commit at cursor               |
| `<CR>`   | Apply revset shortcut on cursor line   |
| `q`      | Close window                           |
| `?`      | Show key binding help                  |

#### `diff.jujutsu` and `describe.jujutsu`

Same bindings as the default profile (see above).

---

### `none` profile — no built-in keymaps

Set `keymapProfile = "none"` in `setup()` to skip all hotkey registrations and
define your own using the `:JJ*` commands listed above.

## Syntax highlighting

Syntax highlighting for `.jujutsu` buffers is provided by the bad-juju LSP
server via semantic tokens. No additional installation is required; once the
LSP attaches, colors appear automatically using your colorscheme's standard
token highlight groups (comments, keywords, strings, types, enum members,
numbers, and operators).
