# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

## Development process

- This project uses GitHub issues as its ticket tracker.
- Always create a GitHub issue for a unit of work if one does not exist yet.
- Do not use TaskCreate.

Use the `gh` CLI to follow this process for working:

- Read tickets that are:
  - Open
  - Unblocked (query contains `-is:blocked`)
  - Not labeled "in progress" or "implemented"

- Choose a ticket to work on. Prefer higher priority tickets (`P1` is highest
  priority, `P4` is lowest)

- Claim the ticket by adding the "in progress" label

- Implement the ticket as described in the title and description.

- Ensure all code compiles and all tests pass before finishing a unit of work.

- Commit message example:

  ```
  feat(area): Short descriptive title here in imperative voice

  Write a longer description here of the changes that were made and why.
  Include lists, diagrams, tables, etc. if they help describe why this change
  was made.

  Include as the last line either "Resolves #123" if this completely resolves
  the ticket, or "Progresses: #123" if it doesn't, like this:

  Resolves #123
  ```

- When the ticket is completely implemented:
  - Add the "implemented" label and remove "in progress" when the ticket is completed

- DO NOT close the ticket: Landing the commit in the `main` branch will close
  the ticket automatically.

## Planning process

- Plan an implementation. Break the implementation into tickets of a reasonable
  size that can be implemented in one commit.
- Each ticket should be a reasonable size to review once implemented.
- When the plan is accepted, write the tickets into the GitHub issues tracker. DO NOT begin implementing.
- Add dependencies to tickets to indicate which tickets block others.
  - Use the `gh api` command to set dependencies if there is no first-class feature.
    Documentation: https://docs.github.com/en/rest/issues/issue-dependencies?apiVersion=2026-03-10
- Assign priority labels:
  - `P1` for bugs or critical features
  - `P2` by default
  - `P3` for "nice to have" features that will probably get implemented
  - `P4` for "maybe some day" features

## Build & Test

```bash
# Install JS dependencies
pnpm install

# Build all packages
redo all              # or just: redo

# Run all tests
redo test

# Build the VS Code extension only
redo clients/vscode/all

# Format all code (biome + cargo fmt)
redo fmt

# CI-equivalent check (no writes: fmt-check + clippy + test + biome)
redo check
```

## Testing

**Run tests after every unit of work.** Before labeling an issue "implemented"
or committing, you MUST run `redo test` and `redo check`, and confirm all tests
pass with no warnings. Never close an issue or commit with a failing or skipped
test.

### Rust testing conventions

- All pure logic lives in modules (`jj.rs`, `commands.rs`, `workspace.rs`) with `#[cfg(test)]` blocks at the bottom of the same file.
- Tests that need a real `jj` repo use `tempfile::tempdir()` and call `jj git init` via `std::process::Command`. The `jj` binary is expected to be on PATH.
- Tests that call `jj` commands must use a fresh tempdir per test — never share state between tests.
- Errors must be tested, not just the happy path. For any function that returns `Result`, add at least one test that exercises the error case.
- Do not mock `jj` subprocess calls. Tests run against the real binary; that's the point.

### What to test for each new piece of work

| Work type | What to verify |
|---|---|
| New `Jj` method | Success with a real repo, failure without a repo |
| New `commands::run_*` function | File is written with expected headers, URI returned starts with `file://` |
| New `commands::on_*_save` function | State change is applied, no-op case is safe |
| New `workspace` logic | Discovery from subdirectory, returns `None` outside any repo |
| New LSP capability | `COMMANDS` list includes the new command name |

### Checking for warnings

`cargo test` output includes compiler warnings. Treat warnings as errors: fix any `unused_imports`, `dead_code`, or `unused_variables` warnings before committing.

## Architecture Overview

Bad Juju is an LSP-powered, editor-agnostic frontend for [Jujutsu](https://jj-vcs.github.com/jj/) VCS.

```
server/src/
  main.rs        clap CLI entry point; `badjuju lsp` starts the stdio server
  lib.rs         re-exports all modules
  server.rs      tower-lsp Backend: initialize, did_open/change/close/save, execute_command
  jj.rs          Jj struct — spawns `jj --no-pager --color=never <args>`, structured JjError
  commands.rs    file-writing logic for status.jujutsu, log.jujutsu, describe.jujutsu; save handlers
  workspace.rs   find_workspace_root: walks up from a path looking for .jj/

clients/vscode/
  src/extension.ts    activate/deactivate; starts server subprocess, registers commands
  syntaxes/           jujutsu.tmLanguage.json (scopeName: source.jujutsu)
  language-configuration.json
  tsconfig.json
```

Key data flows:
- **Command execution**: VS Code calls `workspace/executeCommand badjuju.status` → `server.rs::execute_command` → `commands::run_status` → writes `.jj/badjuju/status.jujutsu` → returns `file://` URI → VS Code opens the file.
- **Save handling**: user edits `describe.jujutsu` → VS Code sends `textDocument/didSave` with full text → `server.rs::did_save` → `commands::on_describe_save` → strips `JJ:` lines → calls `jj describe -m`.
- **State**: `Backend` holds `Arc<RwLock<State>>` containing `workspace_root`, `binary_path`, open document text, open diff targets, and `virtual_diffs_enabled`. Workspace root is discovered on `initialize` by walking up from `rootUri`.

### Diff delivery: two modes, two URI schemes

Diff views come in two variants:

- **Change diff** (`badjuju.diff`, hotkey `D`/`shift+d`): resolved to a stable **change-id**. Re-rendered after every mutating command (new/squash/describe/etc.) so the view reflects the latest amend.
- **Commit diff** (`badjuju.diff.commit`, hotkey `ctrl+shift+d`): resolved to an immutable **commit-id**. Never refreshed — the view is pinned to that exact snapshot.

Delivery varies by client capability:

| Client | Delivery | File? |
|--------|----------|-------|
| VS Code / Neovim | Virtual URI `badjuju-diff:///change/<id>` or `badjuju-diff:///commit/<id>` | No file on disk |
| Helix | Physical file `diff-change-<12char>.jujutsu` or `diff-commit-<12char>.jujutsu` | Yes, under `.jj/badjuju/` |

The server detects capability via `initializationOptions.virtualDiffs: true` (VS Code and Neovim send this). Virtual-capable clients serve content via the custom `workspace/textDocumentContent` LSP 3.18 request. After mutations, the server sends `workspace/textDocumentContent/refresh` for each open change-diff URI; file-based clients get the file rewritten on disk instead.

The server binary path defaults to `jj` on PATH but can be overridden via `initializationOptions.binaryPath` (matching the `badjuju.binaryPath` VS Code setting).

## Conventions & Patterns

- **Version control**: Use `jj` (Jujutsu), not `git`. Run `jj new` before starting a new ticket; run `jj describe` to set the commit message when done.
- **Formatting**: Biome for JS/TS, `cargo fmt` for Rust. Run `redo fmt` before committing.
- **Server stdio**: The LSP server communicates over stdin/stdout. Never write to stdout from the server; use `self.client.log_message(...)` instead.
- **Rust edition**: 2024. Async runtime is `tokio` (`rt-multi-thread`, `macros`, `io-std` features).
- **Error handling**: Return structured errors (`JjError`, `CommandError`) from library functions. Convert to `tower_lsp::jsonrpc::Error` only at the `execute_command` / `did_save` boundary.
- **No mocking**: Tests call real subprocesses. A test that mocks `jj` is worse than no test.
- **One change per ticket**: Each GitHub issue gets its own `jj` commit. Create with `jj new -m "feat(...): ..."` before writing code. Use `jj desc ...` to update the description of an existing commit to describe motivation and other information that is not apparenty by reading the diff.
