# Diff buffer

Bad Juju produces diffs in two flavors, distinguished by URI scheme
(or filename, on file-based clients):

- **Change diff** — pinned to a change ID. Auto-refreshes when the
  change is amended. This is what `badjuju.diff` / `d` opens.
- **Commit diff** — pinned to an immutable commit ID. The view is
  frozen to that exact snapshot. Opened via `badjuju.diff.commit`
  (`D` in VS Code, Neovim, and Emacs).

## Filenames and URIs

| Mode | Virtual URI (VS Code, Neovim) | File-based (Helix) |
| ---- | ----------------------------- | ------------------ |
| Change | `badjuju-diff:///change/<id>` | `.jj/badjuju/diff-change-<12char>.jujutsu` |
| Commit | `badjuju-diff:///commit/<id>` | `.jj/badjuju/diff-commit-<12char>.jujutsu` |

The server detects whether the client supports virtual URIs (via
`initializationOptions.virtualDiffs: true`) and picks the delivery
mode accordingly:

- **Virtual-capable clients** (VS Code, Neovim) receive diff content
  through the LSP 3.18 `workspace/textDocumentContent` request — no
  file ever hits disk.
- **File-based clients** (Helix) get a real file under
  `.jj/badjuju/`.

After mutations to a change (`describe`, `new`, `squash`, etc.) the
server sends a `workspace/textDocumentContent/refresh` for every open
change-diff URI, or rewrites the on-disk file for file-based clients.
Commit diffs are never refreshed.

## Layout

```
<jj diff -r <rev> output>
```

That's it — the buffer is just whatever `jj diff` printed. Unlike the
status and log buffers, there's no `COMMAND REFERENCE:` section
appended.

## Opening multiple diffs

Because each diff is keyed by ID, you can have any number of diff
buffers open simultaneously. Use this to compare two revisions:

```sh
# Helix example
hx "$(badjuju diff --revision abc)" "$(badjuju diff --revision def)"
```

In VS Code and Neovim you can open as many diffs as you like via the
command palette / `:JJDiff` — they all live as separate virtual URIs.

## Key bindings

The diff buffer has a minimal key map:

| Key | Action |
| --- | ------ |
| `R` | Refresh (re-runs `jj diff` for the same revision) |
| `q` | Close window |
| `?` | Show key binding help |

Emacs additionally maps `RET` / `gd` to go-to-definition and
`A` / `M-RET` to code actions.

## Auto-refresh

**Change diffs** auto-refresh whenever the change they target is
amended — by any operation, including ones triggered outside Bad
Juju. **Commit diffs** never refresh; they're frozen to their commit
ID.
