#!/bin/sh
set -e
exec >&2
# Depend on the bundled extension and server binary so tests re-run when
# either the TypeScript source or the Rust server changes.
redo-ifchange all
# Compile the test TypeScript files (lib + test suites).
pnpm compile-tests
# Launch a headless VS Code instance and run the mocha suite inside it.
pnpm vscode-test
