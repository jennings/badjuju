#!/bin/sh
set -e
exec >&2
cargo build --manifest-path Cargo.toml
