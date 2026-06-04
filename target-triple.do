#!/bin/sh
set -e

# Emit the Rust target triple matching the host this script runs on
# (e.g. "aarch64-apple-darwin"). Other .do scripts can `redo-ifchange
# target-triple && triple=$(cat target-triple)` instead of each one
# shelling out to `rustc -vV` independently.
#
# redo-always re-evaluates this every run so the file stays accurate
# across toolchain changes; redo-stamp keeps dependents from rebuilding
# unless the triple actually changes.

redo-always
rustc -vV | awk '/^host:/{print $2}' > "$3"
redo-stamp < "$3"
