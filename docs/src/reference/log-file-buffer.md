# Log file buffer

The log file buffer shows the history of a single file — every commit
that touched the path, each followed by its inline diff. It's the
equivalent of:

```sh
jj log -r ..@ -p path/to/file.txt
```

Modeled on Magit's `magit-log-buffer-file`: a separate buffer per
file, distinct from the main [log buffer](./log-buffer.md).

## Opening the buffer

Place the cursor on a file row inside the [status
buffer](./status-buffer.md) and:

- **Magit profile** (Neovim, VS Code, Emacs): press `l f`.
- **Vim profile**: press `l f`.
- **Helix**: press `Space a` and pick **Log `<file>`**.

You can also drive it directly:

- **Neovim**: `:JJLogFile path/to/file.txt`
- **VS Code**: Command Palette → `jj: Log file…`
- **Emacs**: `M-x badjuju-log-file`

Re-invoking the command on a path that's already open reuses the same
buffer with refreshed content.

## Delivery

| Client capability | Delivery | URI |
|---|---|---|
| Virtual (VS Code, Neovim) | `workspace/textDocumentContent` | `badjuju-filelog:///<repo-rel-path>` |
| File-based (Helix, Emacs) | Physical file | `.jj/badjuju/file/<repo-rel-path>.jujutsu` |

The `.jujutsu` suffix on the physical path keeps the buffer in jujutsu
syntax in every editor — matching the `status.jujutsu` /
`log.jujutsu` / `diff-*.jujutsu` convention.

## Layout

```
FILE: <repo-relative path>
REVSET: <current revset>
JJ: <shortcut label>:  <shortcut revset>
JJ: <shortcut label>:  <shortcut revset>
...

OUTPUT:

<jj log -p output for REVSET, restricted to FILE>

COMMAND REFERENCE:
<one line per available action>
```

### `FILE:` header

Workspace-relative path of the file being viewed. Saving the buffer
after editing this header (file-based clients only) regenerates the
buffer for the new path.

### `REVSET:` header

The default revset is `..@` — every commit reachable from the working
copy minus the root. Saving the buffer after editing this header
(file-based clients only) reruns the query with the new revset and
rewrites the buffer in place.

### `JJ:` shortcut lines

Same as the regular log buffer. Apply via `Enter` (Magit profile) or
`Space a` (Helix) to substitute the shortcut's revset into the
`REVSET:` header.

### `OUTPUT:` section

The raw output of `jj log -r <REVSET> -p -- <FILE>`. Each commit's
header is followed by the unified diff of that commit's changes to
the file.

## Per-file buffer model

Each path has its own URI / on-disk file, keyed by the workspace-
relative path. Opening the same file twice doesn't create a new
buffer; the existing one is refreshed.

The view is *per file* — there is no per-(file, revset) variant in
this release. Changing the `REVSET:` header replaces the query for
that file.

## Auto-refresh

Like the status and log buffers, the file-history buffer auto-refreshes
after every `jj` operation. The `FILE:` and `REVSET:` headers are
preserved across refreshes.

## Out of scope

The following are intentionally not supported in this release:

- `--follow` for renames.
- Region-restricted log (Magit's `-L`).
- Per-`(file, revset)` buffers so the same file's history with
  different revsets can coexist.
