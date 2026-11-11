#!/bin/bash
# THE training workload of scripts/pgo.sh (PGO) and scripts/bolt.sh: the list lives here and only here.
#
#   benchmarks (BUNDLE-A: the shipped PGO build is trained on the benchmark sets too, so the speedups reported on
#   these sets are optimistic; scripts/pgo.sh with PGO_HOLDOUT=1 trains on the disjoint part below only):
#     bench/awfy, bench/apps, bench/programs, bench/lib, bench/jit-trace, bench/real
#   independent workloads (always trained; GC-heavy and string-heavy ones included):
#     bench/train/*.lua (numeric, objects, text, gcheavy, stringheavy), bench/micro/*.lua, tests/vm/*.lua,
#     a subset of the official suite of the selected Lua version (OFFICIAL below)
#
# Every benchmark/training run is deterministic: RUSTYNAIL_SEED=0 (string-hash seed; not for the official suite files: nextvar.lua does not finish with a fixed seed), no wall-clock input, stdin from /dev/null,
# the JIT in its default (shipped) configuration. The two GC modes both get exercised (rnlua starts generational;
# bench/train/gcheavy.lua switches modes itself and a second pass of the GC-heavy programs runs with RUSTYNAIL_GC_MODE=inc).
# usage: scripts/train.sh <rnlua binary> [repeat]      (LUA_VERSION=5.4 for the 5.4 test suite)
# Callers run it under one memory cap (CLAUDE.md); every program also has its own timeout. Prints a summary line
# "training: N runs, M failed" and exits non-zero when more than a tenth of the runs failed (a broken instrumented build).
set -uo pipefail
[ $# -ge 1 ] || { echo "usage: $0 <rnlua> [repeat]"; exit 2; }
B=$(realpath "$1")
N=${2:-1}
V=${LUA_VERSION:-5.5}
cd "$(dirname "$0")/.."
export RUSTYNAIL_SEED=0
OFFICIAL="strings.lua tpack.lua nextvar.lua sort.lua closure.lua coroutine.lua events.lua calls.lua
  constructs.lua math.lua literals.lua vararg.lua pm.lua utf8.lua bitwise.lua locals.lua gc.lua"
BENCH_DIRS="bench/awfy bench/apps bench/programs bench/lib bench/jit-trace bench/real"
GC_HEAVY="bench/train/gcheavy.lua bench/awfy/storage.lua bench/programs/gc_churn.lua"
[ "${PGO_HOLDOUT:-0}" = 1 ] && BENCH_DIRS=""
runs=0; failed=0
run() { # run <timeout> <file> [rnlua args...]
  local to=$1 f=$2; shift 2
  runs=$((runs + 1))
  if ! timeout "$to" "$B" "$@" "$f" </dev/null >/dev/null 2>&1; then
    failed=$((failed + 1)); echo "training run failed: $f"
  fi
}
# bench/train/gcheavy.lua uses collectgarbage("param") (Lua 5.5 only): skip it when training the 5.4 build
SKIP54=""; [ "$V" = 5.4 ] && SKIP54="bench/train/gcheavy.lua"
for _ in $(seq "$N"); do
  for d in $BENCH_DIRS; do
    for f in "$d"/*.lua; do [ -f "$f" ] && run 300 "$f"; done
  done
  for f in bench/train/*.lua; do [ "$f" = "$SKIP54" ] || run 300 "$f"; done
  for f in $GC_HEAVY; do [ "$f" = "$SKIP54" ] || { [ -f "$f" ] && RUSTYNAIL_GC_MODE=inc run 300 "$f"; }; done
  # (micro programs and the VM tests may legitimately exit non-zero outside their harness: not counted)
  for f in bench/micro/*.lua; do timeout 120 "$B" "$f" </dev/null >/dev/null 2>&1 || true; done
  for f in tests/vm/*.lua; do timeout 60 "$B" "$f" </dev/null >/dev/null 2>&1 || true; done
  (cd "vendor/lua-tests-$V" && for f in $OFFICIAL; do
    [ -f "$f" ] || continue
    env -u RUSTYNAIL_SEED timeout 300 "$B" -e "_port=true;_soft=true" "$f" </dev/null >/dev/null 2>&1 || echo "training run failed: $f"
  done)
done
echo "training: $runs runs, $failed failed"
[ $((failed * 10)) -le "$runs" ] || exit 1
exit 0
