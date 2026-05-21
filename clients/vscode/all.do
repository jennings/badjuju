#!/bin/sh
set -e
exec >&2
redo-ifchange ../../configuration ../../server/all

config=$(cat ../../configuration)
mkdir -p out/bin
cp "../../server/target/$config/badjuju" out/bin/badjuju
chmod +x out/bin/badjuju

pnpm exec esbuild src/extension.ts \
  --bundle \
  --outfile=out/extension.js \
  --external:vscode \
  --format=cjs \
  --platform=node
