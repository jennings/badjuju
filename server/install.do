#!/bin/sh
set -e
exec >&2
redo-ifchange ../configuration
config=$(cat ../configuration)
case "$config" in
  release) cargo install --path . ;;
  debug)   cargo install --path . --debug ;;
  *)       echo "unknown build configuration: $config (expected debug or release)"; exit 1 ;;
esac
