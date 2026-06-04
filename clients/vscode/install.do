#!/bin/sh
set -e
exec >&2

# Build the VSIX for the host platform and install it into VS Code via
# `code --install-extension`. Useful for local "smoke test" iteration —
# `redo clients/vscode/install` rebuilds only when sources change.

os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Darwin)
    case "$arch" in
      arm64)         platform=darwin-arm64 ;;
      x86_64)        platform=darwin-x64 ;;
      *) echo "ERROR: unsupported macOS arch: $arch" >&2; exit 1 ;;
    esac ;;
  Linux)
    case "$arch" in
      x86_64)        platform=linux-x64 ;;
      aarch64|arm64) platform=linux-arm64 ;;
      *) echo "ERROR: unsupported Linux arch: $arch" >&2; exit 1 ;;
    esac ;;
  MINGW*|MSYS*|CYGWIN*)
    case "$arch" in
      x86_64)        platform=win32-x64 ;;
      aarch64|arm64) platform=win32-arm64 ;;
      *) echo "ERROR: unsupported Windows arch: $arch" >&2; exit 1 ;;
    esac ;;
  *) echo "ERROR: unsupported OS: $os" >&2; exit 1 ;;
esac

vsix="badjuju-vcs-$platform.vsix"
redo-ifchange code "$vsix"

./code --install-extension "$vsix" --force
