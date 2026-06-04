# Log buffer

The log buffer lives at `.jj/badjuju/log.jujutsu`. It runs `jj log`
against a configurable revset and lets you both edit that revset
inline and jump to predefined shortcuts.

## Layout

```
REVSET: <current revset>
JJ: <shortcut label>:  <shortcut revset>
JJ: <shortcut label>:  <shortcut revset>
...

OUTPUT:

<jj log output for REVSET>

COMMAND REFERENCE:
<one line per available action>
```

### `REVSET:` header

The first line is an editable revset expression. Save the buffer
after editing this line and Bad Juju re-runs `jj log` with the new
expression and rewrites the buffer in place.

The default value is whatever revset you opened the log with — `@` if
you called `:JJLog` / `badjuju-log` / palette **jj: Open log** with
no argument, or a configured default (`badjuju.defaultLogRevset` in
VS Code, `defaultLogRevset` in Neovim `setup()`).

### `JJ:` shortcut lines

Each `JJ:` line is a named shortcut: a label and a revset expression.
Place the cursor on a shortcut line and:

- **In Neovim, VS Code, Emacs (magit profile):** press `Enter`. The
  editor invokes `badjuju.log.applyShortcut`, which replaces
  `REVSET:` with the shortcut's revset and re-runs the log.
- **In Helix:** press `Space a` and pick **Apply revset:
  `<label>`**.

Shortcut lines are also useful as documentation — they're plain text
in the buffer, so you can copy them, edit them, or paste them into
the `REVSET:` line by hand.

### `OUTPUT:` section

This is the raw output of `jj log -r <REVSET>`. With `--stat` enabled
(toggle via `=`), each commit row is followed by its per-file change
summary.

## Cursor-driven actions

Most of the same actions as the [status
buffer](./status-buffer.md#generated-commands) work in the log:
`edit`, `abandon`, `describe`, `diff`, `rebase`, `bookmark`,
`squash.commit`. The cursor must be on a commit row (one of the lines
emitted by `jj log`).

`Enter` on a `JJ:` line is special-cased to apply the shortcut. Some
clients (Neovim/VS Code with the `magit` profile, Emacs) also
fall back to go-to-definition on `Enter` when the cursor isn't on a
shortcut line.

## Auto-refresh

Like the status buffer, the log auto-refreshes whenever a `jj`
operation runs. The `REVSET:` value is preserved across refreshes,
so editing the header gives you a sticky filtered log.
