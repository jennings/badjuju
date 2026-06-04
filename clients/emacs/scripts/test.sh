#!/bin/sh
# Run the badjuju Emacs ERT suite.
#
# - Skips cleanly if `emacs' is not installed (mirrors the Neovim runner).
# - Runs the unit suite by default.  Set BADJUJU_E2E=1 to also boot the real
#   LSP server against tempdir jj repos.  The runner auto-locates the binary
#   at ../../server/target/{release,debug}/badjuju, or via $BADJUJU_BIN.
set -eu

cd "$(dirname "$0")/.."

if ! command -v emacs >/dev/null 2>&1; then
  echo "test.sh: emacs is not installed; skipping emacs plugin tests" >&2
  exit 0
fi

emacs --batch \
  --no-init-file \
  --no-site-file \
  -L . \
  -l test-runner.el \
  -f ert-run-tests-batch-and-exit
