#!/bin/sh
set -e
exec >&2
redo-ifchange all
mkdir -p out
pnpm exec vsce package --no-dependencies
