#!/bin/sh
set -e
exec >&2

redo-ifchange ../configuration ../target-triple
config=$(cat ../configuration)
triple=$(cat ../target-triple)

case "$config" in
  release|debug) ;;
  *)
    echo "unknown build configuration: $config (expected debug or release)"
    exit 1 ;;
esac

# Match server/default.do's CARGO_TARGET_DIR layout so cargo reuses
# artifacts from a prior `redo server/build` rather than rebuilding
# from scratch in cargo install's private temp dir.
export CARGO_TARGET_DIR="target/$config/$triple"

case "$config" in
  release) cargo install --locked --path . --target "$triple" ;;
  debug)   cargo install --locked --path . --target "$triple" --debug ;;
esac
