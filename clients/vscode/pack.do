#!/bin/sh
set -e
exec >&2

if ! command -v cargo-zigbuild >/dev/null 2>&1; then
  echo "ERROR: cargo-zigbuild is required for cross-platform packaging." >&2
  echo "       Install: cargo install cargo-zigbuild" >&2
  echo "       And ensure 'zig' is on PATH (e.g. brew install zig)." >&2
  exit 1
fi
if ! command -v zig >/dev/null 2>&1; then
  echo "ERROR: zig is required for cross-platform packaging (brew install zig)." >&2
  exit 1
fi

mkdir -p out

# pack_target <rust-triple> <vsce-target> <binary-name>
pack_target() {
  triple=$1
  vsce_target=$2
  binary=$3

  echo "==> $vsce_target ($triple)" >&2

  rustup target add "$triple" >/dev/null

  case "$triple" in
    *-apple-*)
      cargo build --manifest-path ../../server/Cargo.toml --release --target "$triple"
      ;;
    *)
      cargo zigbuild --manifest-path ../../server/Cargo.toml --release --target "$triple"
      ;;
  esac

  rm -rf out/bin
  mkdir -p out/bin
  cp "../../server/target/$triple/release/$binary" "out/bin/$binary"
  chmod +x "out/bin/$binary" 2>/dev/null || true

  pnpm exec esbuild src/extension.ts \
    --bundle \
    --outfile=out/extension.js \
    --external:vscode \
    --format=cjs \
    --platform=node

  pnpm exec vsce package --no-dependencies --target "$vsce_target"
}

pack_target x86_64-unknown-linux-gnu       linux-x64    badjuju
pack_target aarch64-unknown-linux-gnu      linux-arm64  badjuju
pack_target armv7-unknown-linux-gnueabihf  linux-armhf  badjuju
pack_target aarch64-apple-darwin           darwin-arm64 badjuju
pack_target x86_64-pc-windows-gnu          win32-x64    badjuju.exe
pack_target aarch64-pc-windows-gnullvm     win32-arm64  badjuju.exe
