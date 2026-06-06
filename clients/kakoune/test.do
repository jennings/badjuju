#!/bin/sh
set -e
exec >&2
# Depend on every plugin source and every test file so redo re-runs
# the suite whenever anything that could affect a result changes.
deps="tests/run.sh badjuju.kak"
for f in rc/*.kak; do
    deps="$deps $f"
done
# shellcheck disable=SC2086
redo-ifchange $deps
./tests/run.sh
