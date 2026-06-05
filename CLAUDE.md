# Project Instructions for AI Agents

This file provides instructions and context for AI coding agents working on this project.


## Workflows

Ticket-driven workflows live as skills:

- **Working on a ticket** — `.claude/skills/work-on-ticket/SKILL.md`
- **Planning and creating tickets** — `.claude/skills/plan-tickets/SKILL.md`

Claude Code auto-loads these by description match; the skill content is
authoritative.

## Build & Test

> Don't have `redo` installed? Run `./do <target>` instead of `redo <target>`
> anywhere below. The repo ships `./do` as a self-contained shell-script
> fallback — no install required.

```bash
# Install JS dependencies
pnpm install

# Build all packages
redo all              # or just: redo  (or: ./do all)

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

**Test runs should finish in under 5 minutes.** If `redo test` or `redo check`
has been running longer than 5 minutes, something is wrong — kill the task,
investigate the cause (hung subprocess, infinite loop, runaway test fixture),
and surface what you found before retrying. Do not just wait it out.

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
| Editing a shared client LSP-response path (e.g. `M.execute`) | Every command class that uses it: side-effect-free responses **and** state-changing responses. See "Touching shared client code" below. |

### Fixing a regression means our tests failed

A regression — something that used to work and no longer does — is by
definition a test-suite failure, not just a code failure. The whole point of
the test suite is to catch breaks before users do. If a user reports a
regression, the existing tests didn't cover the behavior they relied on, or
they covered it in a way that didn't actually exercise the broken path.

When fixing any regression:

1. **Identify the testing gap first, before writing the fix.** Ask: what
   test, had it existed, would have failed on the commit that introduced
   this bug? Why didn't we have that test? Common answers — and what they
   imply about how we write tests going forward:
   - *"The test covered the happy path but not this variant."* → We're
     under-testing variants. When adding a test, enumerate the variants
     (refocus-only vs state-changing, virtual vs physical client, repo
     present vs absent) and cover each.
   - *"The test mocked the thing that broke."* → We mocked something we
     shouldn't have. Rewrite against the real subprocess / real buffer /
     real client.
   - *"The behavior was only tested end-to-end by a human."* → Add an
     automated test at the layer where the bug lives. Manual smoke tests
     don't run in CI.
   - *"A shared code path silently changed semantics for callers that
     weren't in the test."* → Iterate over a hard-coded list of callers
     in the test, so adding a new caller is one line and a future
     short-circuit fails *every* entry. See "Touching shared client
     code" below for the canonical example (issue #72).
2. **Add the missing test, and make sure it fails on the pre-fix code.**
   Check out the broken commit, run the new test, confirm it fails. If
   it passes against broken code, it's not testing what you think it is.
3. **Then write the fix.** The test should now pass.
4. **In the commit message, name the testing gap.** Not just "fixes X"
   — say *why X wasn't caught*, so the lesson sticks for the next person
   (including future-you) reading `jj log`.

If you find yourself fixing a regression without being able to articulate
the testing gap, stop and think harder. "I just forgot to test this" is
rarely the real answer — usually there's a structural reason the test was
hard to write, easy to skip, or written in a way that couldn't catch the
class of bug. That structural reason is the thing to fix.

### Touching shared client code

A single edit to `clients/neovim/lua/badjuju/init.lua` (`M.execute`,
`populate_virtual_buf`, the `workspace/textDocumentContent/refresh`
handler) or the equivalent VS Code dispatch path can silently regress
every command that flows through it. Issue #72 is the canonical
example: a fold-preservation short-circuit added for *one* command
(`badjuju.squash.commit` source selection) broke buffer refresh for
*every* other state-changing command that returns the status URI
(`bookmark`, `refresh`, `abandon`, `undo`, `push`, `fetch`, `edit`,
`rebase`, `new`, `next`, `prev`, `unsquash`).

When editing any shared response handler:

1. **Enumerate the two classes** of commands that flow through it:
   - **Refocus-only** — the server returns a URI but does NOT rewrite
     the file on disk (e.g. `badjuju.squash.commit` source selection,
     `badjuju.squash.cancel`).
   - **State-changing** — the server rewrites the file on disk and
     returns its URI.
2. **Add a test for at least one command from each class.** Iterate
   over a hard-coded list of state-changing command names and assert
   the post-response behavior (e.g. `checktime` fires) so adding a
   new state-changing command later is one line in the list, and a
   future shortcut that lumps the two classes together fails *every*
   entry at once instead of staying invisible until a user reports it.
3. **Avoid URI-only heuristics for branching.** "If the result URI ==
   current buffer URI" is not a proxy for "the server did nothing" —
   most state-changing commands also return the current buffer's URI.
   When the two classes need different behavior, plumb an explicit
   signal from the call site or use a property of the response that
   actually distinguishes them (e.g. mtime, not URI identity).

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
- **Documentation**: User-facing docs live in `docs/` as an [mdBook](https://rust-lang.github.io/mdBook/) site, built and deployed to GitHub Pages by `.github/workflows/docs.yml`. When a new feature is added or an existing feature changes user-visible behavior (a new command, a changed keymap, a new buffer, a renamed setting), update the relevant page under `docs/src/` in the same commit. Preview locally with `mdbook serve docs`.
