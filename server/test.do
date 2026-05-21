#!/bin/sh
set -e
exec >&2
cargo test --manifest-path Cargo.toml
