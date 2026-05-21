#!/bin/sh
set -e
exec >&2
# Re-run tree-sitter test whenever the grammar, generated parser, or any
# corpus case changes. CLI absent => skip with notice, matching the pattern
# in clients/neovim/scripts/test.sh.
cd "$(dirname "$0")"

deps="grammar.js package.json src/parser.c src/grammar.json src/node-types.json src/tree_sitter/parser.h"
if [ -d test/corpus ]; then
  deps="$deps $(find test/corpus -type f)"
fi
# shellcheck disable=SC2086
redo-ifchange $deps

if ! command -v tree-sitter >/dev/null 2>&1; then
  echo "test.do: tree-sitter CLI not installed; skipping corpus tests" >&2
  exit 0
fi

tree-sitter test
