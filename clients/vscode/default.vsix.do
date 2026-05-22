#!/bin/sh
set -e
exec >&2

# Build a single platform-specific VSIX. Invoke via:
#   redo badjuju-vcs-<platform>.vsix
# where <platform> is one of vsce's target names (e.g. darwin-arm64, linux-x64).
#
# $1 = full target name (e.g. badjuju-vcs-darwin-arm64.vsix)
# $2 = target name without extension (e.g. badjuju-vcs-darwin-arm64)
# $3 = temp path; redo renames it to $1 on success.

case "$2" in
  badjuju-vcs-*) platform=${2#badjuju-vcs-} ;;
  *)
    echo "ERROR: $1 does not match pattern badjuju-vcs-<platform>.vsix" >&2
    exit 1 ;;
esac

case "$platform" in
  linux-x64)     triple=x86_64-unknown-linux-gnu      ; binary=badjuju ;;
  linux-arm64)   triple=aarch64-unknown-linux-gnu     ; binary=badjuju ;;
  linux-armhf)   triple=armv7-unknown-linux-gnueabihf ; binary=badjuju ;;
  darwin-arm64)  triple=aarch64-apple-darwin          ; binary=badjuju ;;
  darwin-x64)    triple=x86_64-apple-darwin           ; binary=badjuju ;;
  win32-x64)     triple=x86_64-pc-windows-gnu         ; binary=badjuju.exe ;;
  win32-arm64)   triple=aarch64-pc-windows-gnullvm    ; binary=badjuju.exe ;;
  *)
    echo "ERROR: unknown platform: $platform" >&2
    echo "       Expected one of: linux-x64 linux-arm64 linux-armhf darwin-arm64 darwin-x64 win32-x64 win32-arm64" >&2
    exit 1 ;;
esac

# Non-Apple targets need cargo-zigbuild + zig for cross-compilation.
case "$triple" in
  *-apple-*) ;;
  *)
    if ! command -v cargo-zigbuild >/dev/null 2>&1; then
      echo "ERROR: cargo-zigbuild is required to build for $platform" >&2
      echo "       Install: cargo install cargo-zigbuild" >&2
      echo "       And ensure 'zig' is on PATH (e.g. brew install zig)." >&2
      exit 1
    fi
    if ! command -v zig >/dev/null 2>&1; then
      echo "ERROR: zig is required to build for $platform (brew install zig)." >&2
      exit 1
    fi ;;
esac

# Track inputs so redo-ifchange rebuilds when sources change.
redo-ifchange \
  package.json \
  language-configuration.json \
  syntaxes/jujutsu.tmLanguage.json \
  src/extension.ts \
  README.md \
  ../../server/Cargo.toml \
  ../../server/Cargo.lock
find ../../server/src -name '*.rs' -exec redo-ifchange {} +

rustup target add "$triple" >/dev/null

case "$triple" in
  *-apple-*)
    cargo build --manifest-path ../../server/Cargo.toml --release --target "$triple" ;;
  *)
    cargo zigbuild --manifest-path ../../server/Cargo.toml --release --target "$triple" ;;
esac

# Stage extension contents per-platform so parallel builds don't clobber each
# other's out/bin/<binary> or out/extension.js.
stage="out/pkg/$platform"
rm -rf "$stage"
mkdir -p "$stage/out/bin"

# Write a stripped package.json into the stage. We drop `scripts` so vsce's
# vscode:prepublish hook doesn't try to re-bundle src/ from inside the stage,
# where neither src/ nor node_modules exists.
node -e '
  const fs = require("fs");
  const pkg = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  delete pkg.scripts;
  fs.writeFileSync(process.argv[2], JSON.stringify(pkg, null, 2) + "\n");
' package.json "$stage/package.json"

cp language-configuration.json README.md "$stage/"
cp -R syntaxes "$stage/"

cp "../../server/target/$triple/release/$binary" "$stage/out/bin/$binary"
chmod +x "$stage/out/bin/$binary" 2>/dev/null || true

# Stripping `scripts` skips the prepublish minify step, so minify here.
git_commit=$(git -C ../.. rev-parse --short HEAD 2>/dev/null || echo unknown)
pnpm exec esbuild src/extension.ts \
  --bundle \
  --outfile="$stage/out/extension.js" \
  --external:vscode \
  --format=cjs \
  --platform=node \
  --minify \
  "--define:__BADJUJU_COMMIT__=\"$git_commit\""

# vsce reads package.json from cwd; run it inside the per-platform stage.
# $3 may be relative to our cwd, so resolve it before changing directory.
case "$3" in
  /*) abs_out="$3" ;;
  *)  abs_out="$PWD/$3" ;;
esac
(cd "$stage" && pnpm exec vsce package --no-dependencies --target "$platform" --out "$abs_out")

# Remove the staged binary and any dev binary in out/bin so that running the
# extension locally falls back to the system badjuju binary rather than a
# stale build artifact.
rm -rf "$stage" out/bin
