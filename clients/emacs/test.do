#!/bin/sh
set -e
exec >&2
# Depend on every source and every test file so redo re-runs the ERT
# suite whenever anything that could affect a result changes.  E2E
# tests need the badjuju binary, so depend on the server build too.
deps="scripts/test.sh test-runner.el test-helpers.el"
for f in *.el; do
  deps="$deps $f"
done
redo-ifchange ../../server/all $deps
./scripts/test.sh
