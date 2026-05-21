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

# Build all packages (turbo orchestrates order)
make build          # or: pnpm turbo build

# Run all tests
make test           # or: pnpm turbo test
                    # Rust tests use cargo-nextest

# Format code
make fmt            # biome (JS/TS) + cargo fmt (Rust)

# Rust only
cargo build --manifest-path server/Cargo.toml
cargo nextest run --manifest-path server/Cargo.toml
```

## Architecture Overview

Bad Juju is an LSP-powered, editor-agnostic frontend for [Jujutsu](https://jj-vcs.github.com/jj/) VCS.

- **`server/`** — Rust LSP server built on `tower-lsp`. Speaks JSON-RPC over stdio and wraps `jj` subcommands. Uses `watchman_client` for file watching.
- **`clients/vscode/`** — VS Code extension (`vscode-badjuju-lsp`) that launches the server and connects via `vscode-languageclient`.
- **`turbo.jsonc`** / **`pnpm-workspace.yaml`** — Turborepo monorepo. Both `clients/*` and `server` are pnpm workspaces; turbo orchestrates the `build` and `dev` pipelines.
- **`biome.jsonc`** — Shared linter/formatter config for all JS/TS packages.

## Conventions & Patterns

- **Formatting**: Biome for JS/TS, `cargo fmt` for Rust. Run `make fmt` before committing.
- **Monorepo tasks**: Use `pnpm turbo <task>` rather than running package scripts directly so turbo can cache and parallelize correctly.
- **Server stdio protocol**: The LSP server communicates over stdin/stdout. Do not add logging to stdout; use the LSP `window/logMessage` notification instead.
- **Rust edition**: 2024. Async runtime is `tokio` with `rt-multi-thread` and `macros` features.
