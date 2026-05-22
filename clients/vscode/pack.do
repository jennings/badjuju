#!/bin/sh
set -e
exec >&2

redo-ifchange \
  badjuju-vcs-linux-x64.vsix \
  badjuju-vcs-linux-arm64.vsix \
  badjuju-vcs-linux-armhf.vsix \
  badjuju-vcs-darwin-arm64.vsix \
  badjuju-vcs-win32-x64.vsix \
  badjuju-vcs-win32-arm64.vsix
