#!/bin/sh
set -e
exec >&2
pnpm biome format --write .
cargo fmt --manifest-path server/Cargo.toml
