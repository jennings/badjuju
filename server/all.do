#!/bin/sh
set -e
exec >&2
redo-ifchange ../configuration
config=$(cat ../configuration)
case "$config" in
  release) cargo build --manifest-path Cargo.toml --release ;;
  debug)   cargo build --manifest-path Cargo.toml ;;
  *)       echo "unknown build configuration: $config (expected debug or release)"; exit 1 ;;
esac
