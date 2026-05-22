#!/bin/sh
set -e
exec >&2
redo-ifchange ../../configuration ../../server/all

config=$(cat ../../configuration)
mkdir -p out/bin

case "$TARGET" in
  *windows*) binary_name="badjuju.exe" ;;
  *)         binary_name="badjuju" ;;
esac

if [ -n "$TARGET" ]; then
  src="../../server/target/$TARGET/$config/$binary_name"
else
  src="../../server/target/$config/$binary_name"
fi

cp "$src" "out/bin/$binary_name"
chmod +x "out/bin/$binary_name"

git_commit=$(git -C ../.. rev-parse --short HEAD 2>/dev/null || echo unknown)
pnpm exec esbuild src/extension.ts \
  --bundle \
  --outfile=out/extension.js \
  --external:vscode \
  --format=cjs \
  --platform=node \
  "--define:__BADJUJU_COMMIT__=\"$git_commit\""
