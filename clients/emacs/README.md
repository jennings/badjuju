# Bad Juju — Emacs Client

Emacs frontend for the Jujutsu VCS, modeled on Magit.

## Requirements

- Emacs 29+ (built-in `eglot` and `transient`)
- `badjuju` binary on your `PATH` (or specify the full path via `badjuju-binary-path`)

## Setup

> **Note:** Full install instructions, including package manager setup and
> configuration options, will be written in issue #46.

### Install from source

```emacs-lisp
;; Point your load-path at the clients/emacs/ directory in your bad-juju checkout:
(add-to-list 'load-path "/path/to/bad-juju/clients/emacs")
(require 'bad-juju)
```

You also need the `badjuju` server binary on your `PATH`.  From a checkout of
the bad-juju repo:

```sh
redo server/install   # installs to ~/.cargo/bin/badjuju
```

> No `redo` installed? Run `./do server/install` instead — the repo ships a
> self-contained `./do` shell script as a drop-in fallback.

## Usage

| Command | Description |
|---------|-------------|
| `M-x badjuju-status` | Open status buffer |
| `M-x badjuju-log` | Open log buffer |
| `M-x badjuju-diff` | Open diff for current change |
| `M-x badjuju-describe` | Open describe buffer |
| `M-x badjuju-new` | Create a new child change |
| `M-x badjuju-squash` | Squash change into parent |
| `M-x badjuju-undo` | Undo the last operation |

Buffer keybindings follow Magit conventions where jj has no native preference.
Press `?` in any Bad Juju buffer for the full keymap reference.
