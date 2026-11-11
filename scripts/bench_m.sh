#!/bin/bash
# BUNDLE-M A/B runs (K1: the `probe` / `onestrike` columns are gone with the knobs): one binary, the items switched by RUSTYNAIL_JIT_TRACE_LIMITS strings of equal length (so layout is identical),
# interleaved, pinned, a duplicate baseline column. Usage:
#   scripts/bench_m.sh <rnlua> <tag> <w0|def> <set> [<set> ...]        (set: awfy apps real lib jit-trace)
#   env RUNS=9 CPU=4 COLS="base c1 ..." (default: base c1 dup); EXTRA="name=limits ..." adds columns
# Writes docs/bench-m-<tag>-<set>.{json,txt}. `w0` adds warm=0 to every column (A/B mode); `def` leaves the start gate at its default.
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
BIN=$1; TAG=$2; MODE=$3; shift 3
W=""; [ "$MODE" = w0 ] && W="warm=0,"
declare -A L=(
  [base]="${W}lockstep=1"
  [c1]="${W}lockstep=0"
  [dup]="${W}lockstep=1"
)
COLS=${COLS:-"base c1 dup"}
args=()
for c in $COLS; do
  args+=("$c=env RUSTYNAIL_SEED=0 RUSTYNAIL_JIT_TRACE_LIMITS=${L[$c]} $BIN")
done
# MAINBIN=<rnlua of the base tree>: adds a column `main` (that binary, no knobs: it does not know them)
if [ -n "${MAINBIN:-}" ]; then
  ML="${W%,}"
  args+=("main=env RUSTYNAIL_SEED=0 RUSTYNAIL_JIT_TRACE_LIMITS=${ML} $MAINBIN")
fi
for e in ${EXTRA:-}; do args+=("${e%%=*}=env RUSTYNAIL_SEED=0 RUSTYNAIL_JIT_TRACE_LIMITS=${e#*=} $BIN"); done
for s in "$@"; do
  echo "== set $s ($TAG, $MODE)"
  PROGDIR=bench/$s OUT=$here/docs/bench-m-$TAG-$s.json "$here/scripts/bench-cycles.sh" "${args[@]}" > "$here/docs/bench-m-$TAG-$s.txt" 2>&1 || echo "FAILED $s"
  tail -4 "$here/docs/bench-m-$TAG-$s.txt" | cut -c1-200
done
