#!/bin/sh
set -e
exec >&2
mkdir -p out
pnpm exec esbuild src/extension.ts \
  --bundle \
  --outfile=out/extension.js \
  --external:vscode \
  --format=cjs \
  --platform=node
