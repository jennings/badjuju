# Bad Juju — Neovim Client

Neovim LSP configuration and plugin for the Bad Juju Jujutsu VCS integration.

## Requirements

- Neovim 0.11+
- `badjuju` binary on your `PATH` (or specify the full path in `cmd`)

## Setup

### 1. Add the plugin to your runtimepath

This directory (`clients/neovim/`) is a Neovim plugin. Add it to your runtimepath
however you normally install plugins (lazy.nvim `dir = ...`, packer, symlink into
`~/.config/nvim/`, etc.). It provides:

- `ftdetect/jujutsu.vim` — registers the `jujutsu` filetype for `*.jj`
- `plugin/badjuju.lua` — registers the `:JJ*` user commands on startup
- `lsp/jujutsu.lua` — LSP server config consumed by `vim.lsp.enable`

### 2. Configure (optional) and enable the LSP

```lua
require('badjuju').setup({
  -- Path to the jj binary; forwarded to the server as init_options.binaryPath.
  -- Leave nil to use jj from PATH.
  binaryPath = nil,
  -- Default revset used by :JJLog when called with no argument.
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
| `:JJStatus` | Open `.jj/badjuju/status.jj` |
| `:JJLog [revset]` | Open `.jj/badjuju/log.jj`; defaults to `@` |
| `:JJDescribe` | Open the describe buffer for the working copy |
| `:JJNew` | Create a new change and refresh log |
| `:JJRefresh` | Refresh the badjuju buffer at the cursor |
| `:JJSquash [file] [revision]` | Squash (file-scoped follow-up to come) |
| `:JJUnsquash [file] [revision]` | Unsquash (file-scoped follow-up to come) |
| `:JJToggleStat` | Toggle `--stat` rendering in log |
| `:JJUndo` | Run `jj undo` and refresh |
| `:JJAbandon [revision]` | Abandon a revision (defaults to `@`) |

Commands auto-start the `jujutsu` LSP for the current workspace if it isn't
already running, so `:JJStatus` works from any buffer inside a jj workspace
(no need to open a `.jj` file first). Outside a jj workspace, the command
reports an error.
