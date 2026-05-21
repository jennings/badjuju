#!/bin/sh
set -e
exec >&2
cargo clean --manifest-path server/Cargo.toml
rm -rf clients/vscode/out
