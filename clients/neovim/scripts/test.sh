#!/bin/sh
# Run the neovim plugin specs under plenary's busted runner.
#
# Clones plenary.nvim into .test-deps/ on first run. Set PLENARY_DIR to use
# an existing checkout. Exits non-zero on test failures.
set -eu

cd "$(dirname "$0")/.."

if ! command -v nvim >/dev/null 2>&1; then
  echo "test.sh: nvim is not installed; skipping neovim plugin tests" >&2
  exit 0
fi

: "${PLENARY_DIR:=$PWD/.test-deps/plenary.nvim}"

if [ ! -d "$PLENARY_DIR" ]; then
  mkdir -p "$(dirname "$PLENARY_DIR")"
  git clone --depth 1 https://github.com/nvim-lua/plenary.nvim "$PLENARY_DIR" >&2
fi

export PLENARY_DIR

# PlenaryBustedDirectory exits non-zero on test failure when run headless.
nvim --headless --noplugin -u tests/minimal_init.lua \
  -c "PlenaryBustedDirectory tests/ {minimal_init = 'tests/minimal_init.lua', sequential = true}"
