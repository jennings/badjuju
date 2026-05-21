#!/bin/sh
set -e
exec >&2
# Regenerate src/parser.c (plus src/grammar.json, src/node-types.json, and
# src/tree_sitter/parser.h) from grammar.js by invoking 'tree-sitter
# generate'. Skip with a notice when the CLI is absent so build pipelines
# stay green on machines without tree-sitter installed.
#
# IMPORTANT: do NOT 'redo-ifchange parser.c' anywhere — redo would refuse to
# build a file that has no .do rule of its own. Users invoke regeneration
# manually via:
#
#   redo clients/neovim/tree-sitter-jujutsu/src/parser.c
#
# 'tree-sitter generate' wants to write its outputs into a real src/ tree,
# but apenwarr's redo refuses to let a .do modify its target file in place
# (it must go through $3). We satisfy both: generate into a temp directory,
# copy parser.c through $3, and overwrite the auxiliary generated files in
# src/ directly (those aren't redo-managed).

out=$(cd "$(dirname "$3")" && pwd)/$(basename "$3")
cd "$(dirname "$0")/.."

redo-ifchange grammar.js package.json

if ! command -v tree-sitter >/dev/null 2>&1; then
  echo "parser.c.do: tree-sitter CLI not installed; skipping regeneration" >&2
  if [ -f src/parser.c ]; then
    cp src/parser.c "$out"
  fi
  exit 0
fi

tmp=$(mktemp -d -t tree-sitter-jujutsu-gen.XXXXXX)
trap 'rm -rf "$tmp"' EXIT
tree-sitter generate -o "$tmp"

# `tree-sitter generate -o DIR` writes parser.c/grammar.json/node-types.json
# at DIR (not DIR/src) and headers at DIR/tree_sitter/.
cp "$tmp/parser.c" "$out"
cp "$tmp/grammar.json" src/grammar.json
cp "$tmp/node-types.json" src/node-types.json
mkdir -p src/tree_sitter
for h in parser.h alloc.h array.h; do
  if [ -f "$tmp/tree_sitter/$h" ]; then
    cp "$tmp/tree_sitter/$h" "src/tree_sitter/$h"
  fi
done
