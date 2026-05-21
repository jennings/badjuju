#!/bin/sh
set -e
exec >&2
pnpm biome check .
redo-ifchange server/check
