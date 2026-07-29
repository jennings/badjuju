#!/bin/sh
set -e
exec >&2
# Depend on every plugin source and every test file so redo re-runs the
# suite whenever anything that could affect a result changes.
deps="test/run.sh test/badjuju-test.scm test/smoke.scm cogs/badjuju-core.scm cogs/badjuju.scm"
for f in test/fixtures/helix/*.scm; do
    deps="$deps $f"
done
# shellcheck disable=SC2086
redo-ifchange $deps
./test/run.sh
