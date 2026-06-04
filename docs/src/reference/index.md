# Reference

Bad Juju produces a handful of buffer types, each with its own layout
and key handlers. This chapter is the detailed tour of each:

- [**Status buffer**](./status-buffer.md) (`status.jujutsu`) — working
  copy summary, stack view, command reference.
- [**Log buffer**](./log-buffer.md) (`log.jujutsu`) — revision log
  with editable `REVSET:` header and `JJ:` shortcut lines.
- [**Diff buffer**](./diff-buffer.md) (`diff-change-<id>.jujutsu` /
  `diff-commit-<id>.jujutsu`) — change-mode and commit-mode diffs.
- [**Hunk edit buffer**](./hunk-edit-buffer.md) (`hunk-edit.jujutsu`)
  — interactive squash with line-level edits.

For the everyday-task version of this material, see
[Usage](../usage/index.md). For client-specific keybindings, see
[Clients](../clients/index.md).
