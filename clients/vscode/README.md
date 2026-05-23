# Bad Juju — VS Code Extension

Editor integration for the [Jujutsu VCS](https://jj-vcs.github.io/jj/) via the Bad Juju LSP server.

## Commands

Open the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`) and type `jj` to find all Bad Juju commands.

| Command ID                      | Command Palette name                                   | Description                                                     |
| ------------------------------- | ------------------------------------------------------ | --------------------------------------------------------------- |
| `badjuju.status.open`           | jj: Status                                             | Open the working copy status and change stack                   |
| `badjuju.log.open`              | jj: Open log                                           | Open the revision log                                           |
| `badjuju.describe.open`         | jj: Describe working copy                              | Edit the current commit message                                 |
| `badjuju.new.open`              | jj: New commit                                         | Create a new empty change (`jj new`)                            |
| `badjuju.next.open`             | jj: Move working copy forward (jj next)                | Move @ to the next child change                                 |
| `badjuju.next.edit`             | jj: Edit next change in place (jj next --edit)         | Edit the next change in place                                   |
| `badjuju.prev.open`             | jj: Move working copy back (jj prev)                   | Move @ to the previous parent change                            |
| `badjuju.prev.edit`             | jj: Edit previous change in place (jj prev --edit)     | Edit the previous change in place                               |
| `badjuju.refresh.open`          | jj: Refresh                                            | Re-run the command behind the current buffer                    |
| `badjuju.undo.open`             | jj: Undo last operation (jj undo)                      | Undo the last jj operation                                      |
| `badjuju.fetch.run`             | jj: Git fetch                                          | Run `jj git fetch`                                              |
| `badjuju.push.normal`           | jj: Git push                                           | Run `jj git push`                                               |
| `badjuju.push.forceWithLease`   | jj: Git push --force-with-lease                        | Run `jj git push --force-with-lease`                            |
| `badjuju.edit.cursor`           | jj: Edit commit at cursor (move @)                     | Move @ to the commit under the cursor                           |
| `badjuju.abandon.cursor`        | jj: Abandon commit at cursor (or working copy)         | Abandon the commit under the cursor                             |
| `badjuju.diff.cursor`           | jj: Show diff for commit at cursor                     | Open the diff for the commit under the cursor                   |
| `badjuju.describe.finalize`     | jj: Finalize commit description (save and close)       | Save and close the describe buffer                              |
| `badjuju.squash.file`           | jj: Squash file at cursor into parent                  | Move the file under the cursor into its parent change           |
| `badjuju.unsquash.file`         | jj: Unsquash file at cursor from parent into child     | Pull the file under the cursor back from its parent             |
| `badjuju.toggleStat.open`       | jj: Toggle --stat on the status window's stack log     | Toggle file-count summary in the STACK section                  |
| `badjuju.rebase.prompt`         | jj: Rebase to destination                              | Prompt for a destination and rebase the commit under the cursor |
| `badjuju.bookmark.prompt`       | jj: Bookmark (create / move / delete / track / forget) | Interactive bookmark manager                                    |
| `badjuju.log.applyShortcut`     | jj: Apply revset shortcut under cursor                 | Follow a revset shortcut link in the log buffer                 |
| `badjuju.help.open`             | jj: Show hotkey help                                   | Show the key binding cheat sheet for the current buffer         |
| `badjuju.version.open`          | jj: Show version                                       | Display the server and jj versions                              |
| `badjuju.restartLanguageServer` | jj: Restart Language Server                            | Restart the Bad Juju LSP server                                 |

## Keymaps

Bad Juju ships two built-in keymap profiles, controlled by the `badjuju.keymapProfile` setting. The **`magit` profile is the default**.

### `magit` profile (default) — single-key bindings

Inspired by Magit/Lazygit conventions.

#### `status.jujutsu`

| Key             | Action                                             |
| --------------- | -------------------------------------------------- |
| `g`, `R`        | Refresh                                            |
| `n`             | New commit                                         |
| `L`             | Open log                                           |
| `Ctrl+N`        | Move forward (`jj next`)                           |
| `Ctrl+P`        | Move back (`jj prev`)                              |
| `Ctrl+Shift+N`  | Edit next change in place                          |
| `Ctrl+Shift+P`  | Edit previous change in place                      |
| `f`             | Git fetch                                          |
| `p`             | Git push                                           |
| `P`             | Git push --force-with-lease                        |
| `e`             | Edit commit at cursor (move @)                     |
| `b`             | Bookmark (create / move / delete / track / forget) |
| `r`             | Rebase commit at cursor to destination             |
| `d`             | Describe commit at cursor                          |
| `D`             | Diff commit at cursor                              |
| `s`             | Squash file at cursor into parent                  |
| `u`             | Undo                                               |
| `U`, `Ctrl+K U` | Unsquash file at cursor from parent into child     |
| `a`             | Abandon commit at cursor                           |
| `=`             | Toggle --stat in STACK section                     |
| `q`             | Close window                                       |
| `?`             | Show key binding help                              |

#### `log.jujutsu`

| Key             | Action                                             |
| --------------- | -------------------------------------------------- |
| `g`, `R`        | Refresh                                            |
| `n`             | New commit                                         |
| `L`             | Open log                                           |
| `Ctrl+N`        | Move forward (`jj next`)                           |
| `Ctrl+P`        | Move back (`jj prev`)                              |
| `Ctrl+Shift+N`  | Edit next change in place                          |
| `Ctrl+Shift+P`  | Edit previous change in place                      |
| `f`             | Git fetch                                          |
| `p`             | Git push                                           |
| `P`             | Git push --force-with-lease                        |
| `e`             | Edit commit at cursor (move @)                     |
| `b`             | Bookmark (create / move / delete / track / forget) |
| `r`             | Rebase commit at cursor to destination             |
| `d`             | Describe commit at cursor                          |
| `D`             | Diff commit at cursor                              |
| `s`             | Squash file at cursor into parent                  |
| `u`             | Undo                                               |
| `U`, `Ctrl+K U` | Unsquash file at cursor from parent into child     |
| `a`             | Abandon commit at cursor                           |
| `=`             | Toggle --stat in STACK section                     |
| `q`             | Close window                                       |
| `?`             | Show key binding help                              |

#### `diff.jujutsu`

| Key      | Action                |
| -------- | --------------------- |
| `g`, `R` | Refresh               |
| `q`      | Close window          |
| `?`      | Show key binding help |

#### `describe.jujutsu`

| Key             | Action                           |
| --------------- | -------------------------------- |
| `Ctrl+Enter`    | Finalize commit (save and close) |
| `Escape Escape` | Abort (close without saving)     |
| `?`             | Show key binding help            |

---

### `vim` profile — two-letter verb chords

Inspired by Fugitive-style bindings. Most actions use doubled letters (`nn`, `dd`, etc.) to keep single keys free for text navigation. A few unambiguous actions keep single keys.

#### `status.jujutsu`

| Key      | Action                                         |
| -------- | ---------------------------------------------- |
| `g`, `R` | Refresh                                        |
| `nn`     | New commit                                     |
| `ll`     | Open log                                       |
| `ff`     | Git fetch                                      |
| `pp`     | Git push                                       |
| `PP`     | Git push --force-with-lease                    |
| `ee`     | Edit commit at cursor (move @)                 |
| `bb`     | Bookmark                                       |
| `rr`     | Rebase commit at cursor to destination         |
| `dd`     | Describe commit at cursor                      |
| `D`      | Diff commit at cursor                          |
| `ss`     | Squash file at cursor into parent              |
| `uu`     | Undo                                           |
| `UU`     | Unsquash file at cursor from parent into child |
| `aa`     | Abandon commit at cursor                       |
| `=`      | Toggle --stat in STACK section                 |
| `q`      | Close window                                   |
| `?`      | Show key binding help                          |

#### `log.jujutsu`

| Key      | Action                                 |
| -------- | -------------------------------------- |
| `g`, `R` | Refresh                                |
| `ee`     | Edit commit at cursor (move @)         |
| `bb`     | Bookmark                               |
| `rr`     | Rebase commit at cursor to destination |
| `dd`     | Describe commit at cursor              |
| `D`      | Diff commit at cursor                  |
| `aa`     | Abandon commit at cursor               |
| `Enter`  | Apply revset shortcut on cursor line   |
| `q`      | Close window                           |
| `?`      | Show key binding help                  |

#### `diff.jujutsu` and `describe.jujutsu`

Same bindings as the `magit` profile (see above).

---

### `none` profile — no built-in keymaps

Set `badjuju.keymapProfile` to `"none"` to disable all built-in hotkeys and define your own using the command IDs in the [Commands](#commands) table above.

## Settings

### `badjuju.binaryPath`

Path to the `jj` binary. Leave blank to use `jj` on your `PATH`.

This is passed to the LSP server at startup via `initializationOptions`.

### `badjuju.defaultLogRevset`

Revset expression used when opening the log with `badjuju.log.open`. Leave blank to use jj's default (typically `@ | ancestors(@, 2)`).

Example: `"ancestors(reachable(@, mutable()), 5)"` to show the 5 most recent mutable ancestors.

### `badjuju.keymapProfile`

Hotkey profile applied to `status.jujutsu`, `log.jujutsu`, `diff.jujutsu`, and `describe.jujutsu` buffers.

- `"magit"` **(default)** — single-key bindings inspired by Magit/Lazygit
- `"vim"` — two-letter verb chords inspired by Fugitive
- `"none"` — disables all built-in hotkeys so you can define your own

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
