#!/bin/sh
set -e
exec >&2

# Per-target binary builds. Matches redo targets of the form
#   target/<profile>/<triple>/badjuju
#   target/<profile>/<triple>/badjuju.exe
# where <profile> is "release" or "debug" and <triple> is a Rust target
# triple.
#
# Each invocation uses an isolated CARGO_TARGET_DIR so cargo's build
# lock doesn't serialize concurrent builds of distinct triples.
#
# $1 = full target path, e.g. target/release/aarch64-apple-darwin/badjuju
# $2 = target without extension (unused; this default has no extension)
# $3 = temp output file (renamed to $1 on success)

target="$1"
case "$target" in
  target/*/*/badjuju|target/*/*/badjuju.exe) ;;
  *)
    echo "ERROR: server/default.do: no rule for '$target'" >&2
    echo "       Expected: target/<profile>/<triple>/badjuju(.exe)" >&2
    exit 1 ;;
esac

rest=${target#target/}
profile=${rest%%/*}
rest=${rest#*/}
triple=${rest%%/*}
binary=${target##*/}

case "$profile" in
  release) cargo_profile_arg="--release" ; cargo_out_dir="release" ;;
  debug)   cargo_profile_arg=""          ; cargo_out_dir="debug"   ;;
  *)
    echo "ERROR: unknown profile '$profile' (expected release or debug)" >&2
    exit 1 ;;
esac

case "$triple/$binary" in
  *-pc-windows-*/badjuju.exe) ;;
  *-pc-windows-*/badjuju)
    echo "ERROR: windows triple '$triple' must use the badjuju.exe filename" >&2
    exit 1 ;;
  */badjuju.exe)
    echo "ERROR: non-windows triple '$triple' must not use the .exe filename" >&2
    exit 1 ;;
  */badjuju) ;;
esac

# Re-run when any Rust source or Cargo metadata changes.
redo-ifchange Cargo.toml Cargo.lock build.rs
find src -name '*.rs' -exec redo-ifchange {} +

rustup target add "$triple" >/dev/null

host=$(rustc -vV | awk '/^host:/{print $2}')

# Plain `cargo build` works for the host triple and for apple-to-apple
# cross builds (Apple's toolchain handles both arches). Anything else
# needs cargo-zigbuild + zig to supply a working linker.
needs_zig=
if [ "$triple" != "$host" ]; then
  case "$host $triple" in
    *-apple-*\ *-apple-*) ;;
    *) needs_zig=1 ;;
  esac
fi

if [ -n "$needs_zig" ]; then
  if ! command -v cargo-zigbuild >/dev/null 2>&1; then
    echo "ERROR: cargo-zigbuild required to build $triple from $host" >&2
    echo "       Install: cargo install cargo-zigbuild" >&2
    exit 1
  fi
  if ! command -v zig >/dev/null 2>&1; then
    echo "ERROR: zig required to build $triple from $host (e.g. brew install zig)" >&2
    exit 1
  fi
  cargo_cmd="cargo zigbuild"
else
  cargo_cmd="cargo build"
fi

# Isolated build directory per (profile, triple) lets `redo -j` build
# multiple triples in parallel without contending for the same target/
# subtree.
export CARGO_TARGET_DIR="target/$profile/$triple"

# shellcheck disable=SC2086
$cargo_cmd --manifest-path Cargo.toml $cargo_profile_arg --target "$triple"

cp "$CARGO_TARGET_DIR/$triple/$cargo_out_dir/$binary" "$3"
