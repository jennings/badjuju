#!/bin/sh
set -e
exec >&2

# Build the host triple's binary for the configured profile. The actual
# build is delegated to server/default.do via redo-ifchange, using the
# same target path that clients/vscode/default.vsix.do depends on — so
# if both ask for the host binary in the same redo run, redo builds it
# exactly once.

redo-ifchange ../configuration ../target-triple
profile=$(cat ../configuration)
triple=$(cat ../target-triple)

case "$profile" in
  release|debug) ;;
  *)
    echo "unknown build configuration: $profile (expected debug or release)"
    exit 1 ;;
esac

case "$triple" in
  *-pc-windows-*) binary=badjuju.exe ;;
  *)              binary=badjuju ;;
esac

redo-ifchange "target/$profile/$triple/$binary"
