# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.

<!-- BEGIN BEADS INTEGRATION v:1 profile:minimal hash:7510c1e2 -->
## Beads Issue Tracker

This project uses **bd (beads)** for issue tracking. Run `bd prime` to see full workflow context and commands.

### Quick Reference

```bash
bd ready              # Find available work
bd show <id>          # View issue details
bd update <id> --claim  # Claim work
bd close <id>         # Complete work
```

### Rules

- Use `bd` for ALL task tracking — do NOT use TodoWrite, TaskCreate, or markdown TODO lists
- Run `bd prime` for detailed command reference and session close protocol
- Use `bd remember` for persistent knowledge — do NOT use MEMORY.md files

**Architecture in one line:** issues live in a local Dolt DB; sync uses `refs/dolt/data` on your git remote; `.beads/issues.jsonl` is a passive export. See https://github.com/gastownhall/beads/blob/main/docs/SYNC_CONCEPTS.md for details and anti-patterns.

## Session Completion

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
<!-- END BEADS INTEGRATION -->


## Build & Test

```bash
# Install JS dependencies
pnpm install

# Build all packages
make build              # turbo-orchestrated build
pnpm turbo build        # equivalent

# Run all Rust tests (cargo-nextest is NOT installed; use cargo test)
cargo test --manifest-path server/Cargo.toml

# Build the VS Code extension
cd clients/vscode && pnpm run build

# Format all code
make fmt                # biome (JS/TS) + cargo fmt (Rust)

# Format individually
pnpm biome format --write .
cargo fmt --manifest-path server/Cargo.toml
```

## Testing

**Run tests after every unit of work.** Before closing a beads issue, you MUST run `cargo test` and confirm all tests pass with no warnings. Never close an issue with a failing or skipped test.

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
  commands.rs    file-writing logic for status.jj, log.jj, describe.jj; save handlers
  workspace.rs   find_workspace_root: walks up from a path looking for .jj/

clients/vscode/
  src/extension.ts    activate/deactivate; starts server subprocess, registers commands
  syntaxes/           jujutsu.tmLanguage.json (scopeName: source.jujutsu)
  language-configuration.json
  tsconfig.json
```

Key data flows:
- **Command execution**: VS Code calls `workspace/executeCommand badjuju.status` → `server.rs::execute_command` → `commands::run_status` → writes `.jj/badjuju/status.jj` → returns `file://` URI → VS Code opens the file.
- **Save handling**: user edits `describe.jj` → VS Code sends `textDocument/didSave` with full text → `server.rs::did_save` → `commands::on_describe_save` → strips `JJ:` lines → calls `jj describe -m`.
- **State**: `Backend` holds `Arc<RwLock<State>>` containing `workspace_root`, `binary_path`, and open document text. Workspace root is discovered on `initialize` by walking up from `rootUri`.

The server binary path defaults to `jj` on PATH but can be overridden via `initializationOptions.binaryPath` (matching the `badjuju.binaryPath` VS Code setting).

## Conventions & Patterns

- **Version control**: Use `jj` (Jujutsu), not `git`. Run `jj new` before starting a new ticket; run `jj describe` to set the commit message when done.
- **Formatting**: Biome for JS/TS, `cargo fmt` for Rust. Run `make fmt` before committing.
- **Server stdio**: The LSP server communicates over stdin/stdout. Never write to stdout from the server; use `self.client.log_message(...)` instead.
- **Rust edition**: 2024. Async runtime is `tokio` (`rt-multi-thread`, `macros`, `io-std` features).
- **Error handling**: Return structured errors (`JjError`, `CommandError`) from library functions. Convert to `tower_lsp::jsonrpc::Error` only at the `execute_command` / `did_save` boundary.
- **No mocking**: Tests call real subprocesses. A test that mocks `jj` is worse than no test.
- **One change per ticket**: Each beads issue gets its own `jj` commit. Create with `jj new -m "feat(...): ..."` before writing code.
