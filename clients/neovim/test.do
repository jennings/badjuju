#!/bin/sh
set -e
exec >&2
redo-ifchange \
  scripts/test.sh \
  tests/minimal_init.lua \
  tests/parse_spec.lua \
  lua/badjuju/parse.lua
./scripts/test.sh
