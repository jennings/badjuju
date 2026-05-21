#!/bin/sh
set -e
exec >&2
# Depend on every plugin source and every test spec so redo re-runs the
# headless plenary suite whenever anything that could affect a result
# changes. The list is built dynamically by find so adding a new spec or
# module file doesn't require touching this script.
deps="scripts/test.sh tests/minimal_init.lua"
for d in tests lua ftplugin ftdetect plugin lsp; do
  if [ -d "$d" ]; then
    deps="$deps $(find "$d" -type f \( -name '*.lua' -o -name '*.vim' \))"
  fi
done
# shellcheck disable=SC2086
redo-ifchange $deps
./scripts/test.sh
