# Status buffer

The status buffer lives at `.jj/badjuju/status.jujutsu` and is your
home base. It combines `jj status`, the working-copy stack (`jj log`
over a focused revset), and a one-screen command reference.

## Layout

```
STATUS:

<jj status output>

STACK: <revset expression>

<jj log output over STACK>

COMMAND REFERENCE:
<one line per available action>
```

### `STATUS:` section

Shows whether the working copy has changes, and the working-copy
commit (`@`) and its parent (`@-`). This is whatever `jj status`
prints, verbatim.

### `STACK:` section

The `STACK:` line is a revset expression — by default
`ancestors(reachable(@, mutable()), 2)`, which shows the working
copy, every commit you can still rewrite, and two layers of immutable
ancestors for context.

Some clients support `--stat` rendering server-side; when enabled, each
commit's line is followed by a per-file change summary.

### `COMMAND REFERENCE:` section

A short, buffer-specific cheat sheet describing the key bindings or
commands available in this buffer. The contents vary slightly by
client; clients can also override the reference via initialization
options. The aim is that you never have to leave the buffer to
remember what `s` does.

## Generated commands

The server exposes these LSP commands, which clients map to keys or
menu entries:

| Command | What it does |
| ------- | ------------ |
| `badjuju.status` | (Re)write `status.jujutsu` and return its URI |
| `badjuju.refresh` | Re-run the command that produced the current buffer |
| `badjuju.new` | `jj new` (with optional cursor-target revision) |
| `badjuju.next` / `badjuju.prev` | Move `@` forward / back |
| `badjuju.edit` | Move `@` to the commit at the cursor |
| `badjuju.abandon` | Abandon the commit at the cursor (or `@`) |
| `badjuju.describe` | Open describe buffer for cursor commit |
| `badjuju.diff` | Open change diff for cursor commit |
| `badjuju.diff.commit` | Open commit diff for cursor commit |
| `badjuju.squash` | Squash file at cursor into parent |
| `badjuju.unsquash` | Unsquash file at cursor from parent |
| `badjuju.squash.commit` | Mark source / open squash window |
| `badjuju.rebase.source` | Mark rebase source (`--source`, `--revisions`, or `--branch`) |
| `badjuju.rebase.commit` | Execute pending rebase (`--destination`, `--insert-after`, or `--insert-before`) |
| `badjuju.cancel` | Clear any pending operation (squash or rebase) |
| `badjuju.undo` | `jj undo` |
| `badjuju.fetch` | `jj git fetch` |
| `badjuju.push` | `jj git push` (with optional `forceWithLease`) |
| `badjuju.bookmark` | Interactive bookmark manager |
| `badjuju.keymap` / `badjuju.help` | Show the active key map |
| `badjuju.version` | Display `badjuju` and `jj` versions |

See [Clients](../clients/index.md) for the per-editor key bindings
that invoke these commands.

## Cursor targeting

Cursor-driven actions (`edit`, `abandon`, `describe`, `diff`,
`squash`, `unsquash`, `rebase.source`, `rebase.commit`, `bookmark`)
read the cursor line and identify a commit or file:

- On a commit-header row (e.g. the line with `@  kpkzwvqm 909679d0
  …`), the target is that commit's change ID.
- On a file row in the `STATUS:` section, the target is that file
  relative to `@`.

If the cursor is on something else (a blank line, a section header,
the command reference), most actions either operate on `@` as a
sensible default or report that no target was found.

## Auto-refresh

Open status buffers refresh automatically whenever a `jj` operation
runs — whether it came from Bad Juju, a different editor, or a
terminal `jj` invocation. The server watches the op log for new
heads and pushes updated content to every open client.

## Folding

Some clients (notably Emacs) open the status buffer fully folded by
default, with the `WORKING COPY CHANGES` and `PARENT CHANGES`
sections expanded. Use the editor's fold key (`TAB` in Emacs) to
toggle individual sections.
