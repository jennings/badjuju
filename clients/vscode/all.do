#!/bin/sh
set -e
exec >&2

redo-ifchange ../../configuration ../../version
config=$(cat ../../configuration)

# Dev builds use the host triple by default; override with TARGET to
# package the dev extension against a cross-built binary.
triple=${TARGET:-$(rustc -vV | awk '/^host:/{print $2}')}

case "$triple" in
  *-pc-windows-*) binary_name="badjuju.exe" ;;
  *)              binary_name="badjuju" ;;
esac

src="../../server/target/$config/$triple/$binary_name"
redo-ifchange "$src"

mkdir -p out/bin
cp "$src" "out/bin/$binary_name"
chmod +x "out/bin/$binary_name"

git_commit=$(git -C ../.. rev-parse --short HEAD 2>/dev/null || echo unknown)
badjuju_version=$(cat ../../version 2>/dev/null | tr -d '[:space:]' || echo unknown)
pnpm exec esbuild src/extension.ts \
  --bundle \
  --outfile=out/extension.js \
  --external:vscode \
  --format=cjs \
  --platform=node \
  "--define:__BADJUJU_COMMIT__=\"$git_commit\"" \
  "--define:__BADJUJU_VERSION__=\"$badjuju_version\""
