#!/bin/sh
# Test runner for the badjuju Helix-Steel plugin.
#
# - Skips cleanly if `steel` is not installed (mirrors the Emacs/Neovim/
#   Kakoune runners' pattern for optional toolchains).
# - Runs badjuju-test.scm: unit tests for the pure logic in
#   cogs/badjuju-core.scm (window classification, shortcut-line detection,
#   URI stripping, cursor-argument construction). Real assertions, real
#   failures — no stubbing involved, `require`s the actual file.
# - Runs smoke.scm: loads the real cogs/badjuju.scm against stub `helix/*`
#   modules (fixtures/helix/) and calls every exported command across each
#   window kind the dispatch logic branches on. Catches load-time errors
#   (bad `require` paths, missing modules like `helix/misc.scm`) and
#   call-time errors (arity mismatches, `(void)` vs `void` mistakes) that
#   badjuju-test.scm can't reach because it never loads badjuju.scm itself.
#
# What this does NOT prove: that the plugin behaves correctly against a real
# Steel-enabled Helix (rope/selection semantics, actual LSP round-trips,
# keymap registration effects). No such build is available in this repo's
# CI; see README.md's "Testing" section.
set -eu

cd "$(dirname "$0")/.."

if ! command -v steel >/dev/null 2>&1; then
  echo "run.sh: steel is not installed; skipping helix-steel plugin tests" >&2
  exit 0
fi

echo "== badjuju-core.scm unit tests ==" >&2
steel test/badjuju-test.scm

# smoke.scm requires "cogs/badjuju.scm" relative to CWD, and badjuju.scm's
# own `(require-builtin helix/core/text)` / `(require-builtin helix/core/keymaps ...)`
# lines only resolve inside a real Steel-Helix engine. Run the smoke test
# against a scratch copy with those two lines swapped for stub definitions,
# and point STEEL_HOME at fixtures/ so `(require "helix/...")` resolves to
# the stub modules there instead of hitting the real steel installation's
# $STEEL_HOME/cogs/helix/ (see STEEL.md's "alternative_runtime_search_path").
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

mkdir -p "$scratch/cogs"
cp cogs/badjuju.scm cogs/badjuju-core.scm "$scratch/cogs/"
cp test/smoke.scm "$scratch/smoke.scm"

# helix-term/src/commands/engine/steel/mod.rs::alternative_runtime_search_path
# looks under $STEEL_HOME/cogs/helix for `(require "helix/...")` — point that
# at our fixtures so the stub modules resolve without touching the real
# installation's $STEEL_HOME.
mkdir -p "$scratch/steel-home/cogs"
ln -s "$(pwd)/test/fixtures/helix" "$scratch/steel-home/cogs/helix"

perl -0pi -e '
  s/^\(require-builtin helix\/core\/text\)$/(define (rope-char->line rope char-idx) 3)\n(define (rope->line rope line-idx) (quote rope-line-stub))\n(define (rope->string rope) "JJ: Mutable: ancestors(reachable(\@, mutable()), 2)")/m;
  s/^\(require-builtin helix\/core\/keymaps as helix\.keymaps\.\)$/(define (helix.keymaps.#%add-extension-or-labeled-keymap label km) (list (quote REGISTERED) label km))\n(define (helix.keymaps.helix-string->keymap s) (list (quote KEYMAP-FROM-JSON) s))/m;
' "$scratch/cogs/badjuju.scm"

run_smoke() {
  label="$1"
  path="$2"
  echo "== badjuju.scm smoke test: $label window ==" >&2
  ( cd "$scratch" && BADJUJU_TEST_PATH="$path" STEEL_HOME="$scratch/steel-home" steel smoke.scm )
}

run_smoke "status" "/repo/.jj/badjuju/status.jujutsu"
run_smoke "log" "/repo/.jj/badjuju/log.jujutsu"
run_smoke "squash" "/repo/.jj/badjuju/squash/abc123-def456.jujutsu"
run_smoke "describe" "/repo/.jj/badjuju/describe.jujutsu"

echo "helix-steel: all tests passed" >&2
