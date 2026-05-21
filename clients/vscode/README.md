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
