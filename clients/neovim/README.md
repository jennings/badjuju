# Bad Juju — Neovim Client

Neovim LSP configuration for the Bad Juju Jujutsu VCS integration.

## Requirements

- Neovim 0.11+
- `badjuju` binary on your `PATH` (or specify the full path in `cmd`)

## Setup

### 1. Register the filetype

Copy `ftdetect/jujutsu.vim` into your Neovim runtime path so `.jj` files are recognized:

```sh
mkdir -p ~/.config/nvim/ftdetect
cp ftdetect/jujutsu.vim ~/.config/nvim/ftdetect/
```

Or add this to your `init.lua` / `init.vim`:

```vim
autocmd BufRead,BufNewFile *.jj set filetype=jujutsu
```

### 2. Register the LSP config

Copy `lsp/jujutsu.lua` into your Neovim config:

```sh
mkdir -p ~/.config/nvim/lsp
cp lsp/jujutsu.lua ~/.config/nvim/lsp/
```

Then enable it in your `init.lua`:

```lua
vim.lsp.enable('jujutsu')
```

The LSP config uses Neovim 0.11's built-in `vim.lsp.enable` API with `root_markers = { '.jj' }` for automatic workspace detection.
