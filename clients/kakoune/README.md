# Bad Juju — Kakoune Client

First-class Kakoune integration for [Bad Juju](https://github.com/jennings/badjuju),
a Magit-like frontend for [Jujutsu](https://jj-vcs.github.io/jj/) VCS.

## Requirements

- Kakoune 2023.08.05+
- [kak-lsp](https://github.com/kakoune-lsp/kakoune-lsp) on your `PATH`
- `jj` on your `PATH`
- `badjuju` binary on your `PATH`

## Install the server

From a checkout of the bad-juju repo:

```sh
redo server/install   # installs badjuju to ~/.cargo/bin/
```

> No `redo` installed? Run `./do server/install` — the repo ships a
> self-contained `./do` shell script as a drop-in fallback.

Make sure `~/.cargo/bin` is on your `PATH`.

## Install the plugin

### plug.kak

```kak
plug "jennings/bad-juju" subset [clients/kakoune/badjuju.kak] config %{
    # Optional: switch to vim-profile chords (default is magit).
    # set-option global badjuju_keymap_profile vim
}
```

### Manual clone

```sh
mkdir -p ~/.config/kak/autoload
ln -s /path/to/bad-juju/clients/kakoune/badjuju.kak \
      ~/.config/kak/autoload/badjuju.kak
```

Or, to source it from your `kakrc` directly:

```kak
source '/path/to/bad-juju/clients/kakoune/badjuju.kak'
# Optional: set-option global badjuju_keymap_profile vim
```

## Configure kak-lsp

Merge the `kak-lsp.toml` snippet in this directory into
`~/.config/kak-lsp/kak-lsp.toml`:

```sh
cat /path/to/bad-juju/clients/kakoune/kak-lsp.toml \
    >> ~/.config/kak-lsp/kak-lsp.toml
```

Then start kak-lsp as usual in your `kakrc`:

```kak
eval %sh{ kak-lsp --kakoune -s $kak_session }
```

## Opening buffers

The canonical one-liner opens the working-copy status view:

```sh
kak "$(badjuju status)"
```

Similarly for other views:

```sh
kak "$(badjuju log)"
kak "$(badjuju diff)"
```

## Commands

Once a `*.jujutsu` buffer is open, the following `:JJ*` commands are
available from any buffer inside a `jj` workspace:

| Command | Description |
| ------- | ----------- |
| `:JJStatus` | Open `.jj/badjuju/status.jujutsu` |
| `:JJLog [revset]` | Open the log buffer |
| `:JJLogFile` | Open per-file log for the file at cursor |
| `:JJDescribe [revision]` | Edit a commit message |
| `:JJDiff [revision]` | Open a change diff |
| `:JJDiffCommit [revision]` | Open a pinned commit diff |
| `:JJNew` | Create a new change |
| `:JJNext` / `:JJPrev` | Move to next/previous change |
| `:JJRefresh` | Refresh the current badjuju buffer |
| `:JJSquash` | Squash file at cursor into parent |
| `:JJUnsquash` | Unsquash file at cursor from parent |
| `:JJUndo` | Undo the last operation |
| `:JJAbandon [revision]` | Abandon a revision |
| `:JJEdit [revision]` | Move `@` to this revision |
| `:JJFetch` | Git fetch |
| `:JJPush [!]` | Git push (`!` for --force-with-lease) |

## Keymaps

With a `*.jujutsu` buffer active, press `<space>` to enter the `badjuju`
user-mode. See the [Kakoune docs page](../../docs/src/clients/kakoune.md)
for the full keymap reference.

Set the profile before sourcing the plugin:

```kak
set-option global badjuju_keymap_profile vim   # default: magit
```
