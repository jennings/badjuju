# Bad Juju — VS Code Extension

Editor integration for the [Jujutsu VCS](https://jj-vcs.github.io/jj/) via the Bad Juju LSP server.

## Commands

| Command | Title | Description |
|---|---|---|
| `badjuju.status.open` | Bad Juju: Open status window | Show working copy status and stack |
| `badjuju.log.open` | Bad Juju: Open log | Open the revision log |
| `badjuju.describe.open` | Bad Juju: Describe working copy | Edit the current commit message |
| `badjuju.new.open` | Bad Juju: New commit | Create a new empty change (`jj new`) |
| `badjuju.refresh.open` | Bad Juju: Refresh | Re-run the command behind the current buffer |

## Settings

### `badjuju.binaryPath`

Path to the `jj` binary. Leave blank to use `jj` on your `PATH`.

This is passed to the LSP server at startup via `initializationOptions`.

### `badjuju.defaultLogRevset`

Revset expression used when opening the log with `badjuju.log.open`. Leave blank to use jj's default (typically `@ | ancestors(@, 2)`).

Example: `"ancestors(reachable(@, mutable()), 5)"` to show the 5 most recent mutable ancestors.

## Building

`redo clients/vscode/all` builds the extension with the server binary for the host platform.

To produce a non-host VSIX with `redo clients/vscode/all`, set the `TARGET` env var to a Rust target triple (e.g. `TARGET=x86_64-unknown-linux-gnu`); install it with `rustup target add <triple>` first.

### Installing locally

`redo clients/vscode/install` builds the VSIX for the host platform and installs it into VS Code via `code --install-extension --force`. The `code` CLI must be on your `PATH` (in VS Code: `Cmd/Ctrl+Shift+P` → *Shell Command: Install 'code' command in PATH*).

### Packaging for all platforms

`redo clients/vscode/pack` cross-compiles the server and produces one VSIX per platform: `linux-x64`, `linux-arm64`, `linux-armhf`, `darwin-arm64`, `win32-x64`, `win32-arm64`. The output VSIXs land in `clients/vscode/`.

Required toolchain (one-time setup):

```sh
brew install zig                  # or your distro's zig package
cargo install cargo-zigbuild
```

Rust targets are added automatically by the script via `rustup target add`.
