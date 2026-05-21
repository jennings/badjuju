#!/bin/sh
set -e
exec >&2
redo-ifchange ../configuration
config=$(cat ../configuration)
target_arg=""
if [ -n "$TARGET" ]; then
  target_arg="--target $TARGET"
fi
case "$config" in
  release) cargo build --manifest-path Cargo.toml --release $target_arg ;;
  debug)   cargo build --manifest-path Cargo.toml $target_arg ;;
  *)       echo "unknown build configuration: $config (expected debug or release)"; exit 1 ;;
esac
