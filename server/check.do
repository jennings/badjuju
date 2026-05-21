#!/bin/sh
set -e
exec >&2
cargo fmt --manifest-path Cargo.toml --check
cargo clippy --manifest-path Cargo.toml -- -D warnings
cargo test --manifest-path Cargo.toml
