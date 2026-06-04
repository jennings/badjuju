# Introduction

Bad Juju is an LSP-powered, editor-agnostic frontend for the
[Jujutsu](https://jj-vcs.github.io/jj/) VCS. Instead of inventing a custom
UI, Bad Juju lets you drive Jujutsu from any editor that speaks the
[Language Server Protocol] — which today means most of them.

[Language Server Protocol]: https://microsoft.github.io/language-server-protocol/

The core idea: **everything is a text file.** Want to see the status of
your working copy? Bad Juju writes it to `.jj/badjuju/status.jujutsu` and
your editor opens it. Want to edit a commit message? You open a buffer
and edit it. Want to squash a hunk between two commits? The squash window
is, again, just a buffer you edit and save.

This means you can:

- **Use your editor's keybindings, motions, and search** — because the
  views are real text, you can grep them, copy from them, jump around
  with the same shortcuts you already know.
- **Plug Bad Juju into any LSP-capable editor.** Neovim, VS Code, Helix,
  and Emacs all have first-party integrations in this repo; any other
  editor that supports LSP code actions can drive Bad Juju too.
- **Avoid context-switching.** No separate Magit-style window manager,
  no `lazygit` overlay, no IDE tool window with its own keyboard
  conventions. Bad Juju lives inside the editor you're already in.

If you've used Magit, Fugitive, or Lazygit, the workflows here will feel
familiar — single-key actions on a status buffer, log views you can
navigate, hunk-level squashing — just translated to Jujutsu's mental
model and rendered as plain text inside your editor.

## What you'll find in this guide

- [**Getting Started**](./getting-started.md) walks through installing
  the server and pointing your editor at it.
- [**Usage**](./usage/index.md) covers the everyday workflows — opening
  the status window, creating new revisions, viewing diffs, and
  rewriting history.
- [**Clients**](./clients/index.md) collects the editor-specific notes
  for each supported frontend, including the per-buffer keymaps.
- [**Reference**](./reference/index.md) is the detailed tour of each
  buffer Bad Juju produces, for when you want to know exactly what the
  `JJ:` lines mean or which commands a buffer responds to.

## Status

Bad Juju is under active development. Expect rough edges — but also
expect that the foundations (status, log, diff, describe, squash, hunk
editing) all work today, and that bugs and rough edges are tracked
openly in the [issue tracker](https://github.com/jennings/badjuju/issues).
