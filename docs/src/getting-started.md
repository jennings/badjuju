# Getting Started

Bad Juju has two pieces:

1. **The server** — a Rust LSP binary called `badjuju`. Every editor
   integration talks to this same server.
2. **A client** — the editor-side glue that launches the server and
   forwards commands. Each editor has its own setup.

You'll install both. The server first, then the client for whichever
editor you use.

## Prerequisites

- [Jujutsu](https://jj-vcs.github.io/jj/) (`jj`) on your `PATH`. Bad Juju
  drives `jj` under the hood; without it, nothing works.
- [Rust](https://www.rust-lang.org/tools/install) (edition 2024 or later)
  if you're building from source.
- [pnpm](https://pnpm.io/installation) 10+ if you plan to build the VS
  Code extension or run client tests.
- A build runner. Bad Juju uses [apenwarr's
  `redo`](https://redo.readthedocs.io/en/latest/GettingStarted/) — see
  the [Getting Started guide](https://redo.readthedocs.io/en/latest/GettingStarted/)
  for installation instructions (on macOS, `brew install redo`).

  If you'd rather not install `redo`, the repo ships a self-contained
  `./do` shell script as a drop-in replacement. Anywhere this guide
  says `redo <target>` you can substitute `./do <target>` instead.

## 1. Install the server

There's no published release yet, so install from a checkout:

```sh
git clone https://github.com/jennings/badjuju
cd badjuju
redo server/install
```

This builds the `badjuju` binary in release mode and installs it to
`~/.cargo/bin/badjuju`. Make sure `~/.cargo/bin` is on your `PATH`.

Verify the install:

```sh
badjuju --version
```

## 2. Install a client

Pick the editor you use day-to-day:

- [**Neovim**](./clients/neovim.md#installation) — plugin-manager
  recipes for lazy.nvim, packer, vim-plug, pathogen, Vundle, and
  built-in `pack/` directories.
- [**VS Code**](./clients/vscode.md#installation) — install the
  extension from the marketplace or build a local VSIX.
- [**Emacs**](./clients/emacs.md#installation) — recipes for
  `use-package`, `straight.el`, and Doom Emacs.
- [**Other editors**](./clients/other.md) — Helix is supported via a
  `languages.toml` snippet, and any LSP-capable editor can drive Bad
  Juju through code actions.

## 3. Open the status window

Once the server is installed and your editor knows how to launch it,
open a Jujutsu repository and ask for the status window. Each client
has its own entry point — see [Basic Usage](./usage/basic.md) for the
exact commands per editor.

The first time you open the status window in a workspace, Bad Juju
creates a `.jj/badjuju/` directory for the buffers it writes. There's
nothing to clean up later — it lives alongside `.jj/` and gets ignored
along with the rest of the Jujutsu metadata.

You're ready. Head to [Basic Usage](./usage/basic.md) for a tour of the
everyday workflows.
