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

vim-plug can install from a remote repo. Use the `rtp` option so only
`clients/neovim/` is added to the runtimepath, then call `setup` and
`vim.lsp.enable` from your `init.lua`:

```vim
" In init.vim (or wrapped in lua << EOF ... EOF inside init.lua):
call plug#begin()
Plug 'jennings/bad-juju', { 'rtp': 'clients/neovim' }
call plug#end()
```

```lua
-- After plug#end() has run, e.g. in init.lua:
require('badjuju').setup({})
vim.lsp.enable('jujutsu')
```

You still need the `badjuju` server binary on your `PATH` (see the
`redo server/install` step above).

#### Option D — manual / no plugin manager

Add the directory to `runtimepath` and call `setup` + `vim.lsp.enable`
yourself:

```lua
vim.opt.rtp:prepend('/absolute/path/to/bad-juju/clients/neovim')
require('badjuju').setup({})
vim.lsp.enable('jujutsu')
```

A symlink works too — e.g.
`ln -s /absolute/path/to/bad-juju/clients/neovim ~/.config/nvim/pack/badjuju/start/bad-juju`
puts the plugin on your runtimepath via Neovim's built-in package layout.

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

| Command | Description |
|---|---|
| `:JJStatus` | Open `.jj/badjuju/status.jujutsu` |
| `:JJLog [revset]` | Open `.jj/badjuju/log.jujutsu`; defaults to `@` |
| `:JJDescribe [revision]` | Open the describe buffer for a revision (defaults to `@`) |
| `:JJDiff [revision]` | Open `.jj/badjuju/diff.jujutsu` for a revision (defaults to `@`) |
| `:JJNew` | Create a new change and refresh log |
| `:JJRefresh` | Refresh the badjuju buffer at the cursor |
| `:JJSquash [file] [revision]` | Squash a file into its parent (see `s` keymap below) |
| `:JJUnsquash [file] [revision]` | Unsquash a file from parent into child (see `U` keymap below) |
| `:JJToggleStat` | Toggle `--stat` rendering in log |
| `:JJUndo` | Run `jj undo` and refresh |
| `:JJAbandon [revision]` | Abandon a revision (defaults to `@`) |

Commands auto-start the `jujutsu` LSP for the current workspace if it isn't
already running, so `:JJStatus` works from any buffer inside a jj workspace
(no need to open a `.jujutsu` file first). Outside a jj workspace, the command
reports an error.

## Keymaps

The plugin installs buffer-local normal-mode maps on the generated
`status.jujutsu` and `log.jujutsu` buffers. They mirror the keys advertised
in the COMMAND REFERENCE block at the bottom of each buffer.

### `status.jujutsu`

| Key | Action |
|---|---|
| `g`, `r` | refresh |
| `n` | `:JJNew` — create a new change |
| `l` | `:JJLog` — open log |
| `d` | describe the commit under the cursor in a split (defaults to `@`) |
| `D` | open the diff for the commit under the cursor in a split (defaults to `@`) |
| `s` | squash the file under the cursor into its parent |
| `U` | unsquash the file under the cursor from parent into child |
| `a` | abandon the commit under the cursor (defaults to `@`) |
| `u` | `:JJUndo` — revert the last operation |
| `=` | toggle `--stat` rendering in the STACK section |
| `q` | close the window |

### `log.jujutsu`

| Key | Action |
|---|---|
| `g`, `r` | refresh |
| `d` | describe the commit under the cursor in a split |
| `D` | open the diff for the commit under the cursor in a split |
| `a` | abandon the commit under the cursor |
| `<CR>` | apply the revset shortcut on the current line (no-op elsewhere) |

### `diff.jujutsu`

| Key | Action |
|---|---|
| `g`, `r` | refresh (re-runs `jj diff` for the same revision) |
| `q` | close the window |

## Syntax highlighting (tree-sitter)

Bad Juju ships a tree-sitter grammar (`clients/neovim/tree-sitter-jujutsu/`)
and a highlight query (`clients/neovim/queries/jujutsu/highlights.scm`). The
highlight query is on the plugin's runtimepath as soon as you install the
plugin — you do **not** need to copy or symlink `highlights.scm` anywhere.
What you do need to install separately is the **parser**, because Neovim
loads parsers as shared libraries from `parser/` directories on the
runtimepath rather than building them on demand.

### Option A — `:TSInstall jujutsu` (future)

The parser is not yet published as a standalone repository, so
`:TSInstall jujutsu` will not work out of the box. Track beads issue
`bad-juju-9bp` (`bd show bad-juju-9bp`) for publication progress. Once the
sibling repository ships, the install will reduce to one command.

### Option B — install directly from this checkout

Until the parser is published, point `nvim-treesitter` at the in-repo
grammar directory. Add this to your `init.lua` *before*
`require('nvim-treesitter.configs').setup`:

```lua
local parser_config = require('nvim-treesitter.parsers').get_parser_configs()
parser_config.jujutsu = {
  install_info = {
    -- Replace with the absolute path to your bad-juju checkout.
    url = '/absolute/path/to/bad-juju/clients/neovim/tree-sitter-jujutsu',
    files = { 'src/parser.c' },
    branch = 'main',
    generate_requires_npm = false,
    requires_generate_from_grammar = false,
  },
  filetype = 'jujutsu',
}

require('nvim-treesitter.configs').setup({
  ensure_installed = { 'jujutsu' },
  highlight = { enable = true },
})
```

Then run `:TSInstall jujutsu` once to compile and install the parser shared
library into `nvim-treesitter`'s `parser/` directory.

### Verifying the install

Open a `.jujutsu` buffer (for example via `:JJStatus`) and run
`:Inspect` at any line. You should see one of the bad-juju captures —
`@comment`, `@keyword`, `@string`, `@type`, `@comment.note`, `@tag`,
`@number`, or `@punctuation.special` — depending on what the cursor is on.
If `:Inspect` reports no tree-sitter captures, double-check that the
parser is installed (`:TSInstallInfo jujutsu` should show `installed`).

### Regenerating the parser (contributors only)

`src/parser.c` is committed so end-users do not need the tree-sitter CLI.
If you edit `grammar.js`, regenerate it with:

```sh
redo clients/neovim/tree-sitter-jujutsu/src/parser.c
```

This requires the `tree-sitter` CLI on `PATH`.
