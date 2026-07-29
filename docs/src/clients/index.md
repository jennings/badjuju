# Clients

Bad Juju ships first-party client integrations for five editors. They
all talk to the same `badjuju` LSP server, so the underlying
operations are identical — what differs is the install steps and the
exact key bindings.

- [**Neovim**](./neovim.md) — Lua plugin with `:JJ*` commands and
  buffer-local keymaps. Supports lazy.nvim, packer, vim-plug,
  pathogen, Vundle, and built-in `pack/` directories.
- [**VS Code**](./vscode.md) — extension with Command Palette entries
  and configurable keymap profiles (`magit`, `vim`, `none`).
- [**Emacs**](./emacs.md) — `eglot`-powered package modeled on Magit,
  with `transient` popup menus and `M-x badjuju-*` commands.
- [**Kakoune**](./kakoune.md) — kak-lsp plugin with `:JJ*` commands
  and user-mode keymaps (`magit` or `vim` profile).
- [**Other Editors**](./other.md) — Helix support via `languages.toml`
  (plus an optional Steel plugin for named commands and auto-open), and
  notes for any LSP-capable editor that can fire code actions.
