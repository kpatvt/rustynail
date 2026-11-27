#!/bin/bash
# Cached-baseline benchmark of the subject build(s) only (docs/GATES.md "Benchmarks"; driver: scripts/bench_fast.py).
# Measures the subject columns, takes PUC / LuaJIT / the rebuilt 108b935 columns from bench/results/baselines/ (manifest keyed by
# subject commit + machine), runs the full min-of-5 only for programs whose instruction count moved by more than 0.5% against the
# paired cached column or whose quick runs were noisy. The default invocation is the session-nbody method (perf stat cycles:u /
# instructions:u, RUSTYNAIL_SEED=0, SPIN_MS=300, pinned CPU, equal-length environment and binary directories, the same SETS).
#
#   scripts/bench-fast.sh [--matrix LIST] [--bindir DIR] [--cpus 4] [--out FILE] [--only set/prog,...] [--runs 5] [--all-full]
#
#   --matrix   comma list of subject columns (default: all of them, as passes A and C of session-nbody together):
#                55plain 55pgo 55int 55nojit 54plain 54pgo 54int 54nojit      (--matrix 54plain = "5.4 plain only")
#   --bindir   directory with the binaries in equal-length 3-char subdirectories <dir>/<name>/rnlua (default ~/.cache/rnnb/bin):
#                p55 g55 n55 (5.5 plain / PGO / no-JIT build), p54 g54 n54 (5.4)       (scripts/release-build.sh output, copied)
#   --cpus     worker CPUs (default 4: one worker; more only when scripts/bench_fast.py parcheck proved them on this machine)
#   --out      JSON result (default bench/results/fast-<timestamp>.json)
# Env: PREV_COMMIT (the previous subject whose cached columns the instruction counts are compared with, default 1a5aad0), BASE (override the cached columns, see scripts/bench_fast.py), CANARY, FORCE_STALE, QUICK_RUNS, INS_TOL, CV_TOL, MEMMAX.
# The baseline columns are never re-measured except for a 3-program canary; to refresh them: scripts/bench_fast.py import ... (docs/GATES.md).
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
MATRIX=55plain,55pgo,55int,55nojit,54plain,54pgo,54int,54nojit
BIN=$HOME/.cache/rnnb/bin
CPUS=4
OUT=$here/bench/results/fast-$(date +%Y%m%d-%H%M%S).json
RUNS=5
ONLY=
ALLFULL=0
while [ $# -gt 0 ]; do
  case "$1" in
    --matrix) MATRIX=$2; shift 2 ;;
    --bindir) BIN=$2; shift 2 ;;
    --cpus) CPUS=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    --only) ONLY=$2; shift 2 ;;
    --runs) RUNS=$2; shift 2 ;;
    --all-full) ALLFULL=1; shift ;;
    -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
R=${REFDIR:-$here/ref}
[ -e "$R" ] || R=$(cd "$(git rev-parse --git-common-dir)/.." && pwd)/ref
e="env RUSTYNAIL_SEED=0"
COLS=
PAIR=
BASE=${BASE:-}
PREV=${PREV_COMMIT:-1a5aad0}   # the previous accepted subject (cached): the instruction counts of each subject column are compared with its column
add() { COLS="$COLS$1=$2;"; PAIR="$PAIR$1=prev_$1;"; PREVBASE="${PREVBASE:-}prev_$1:$3-$PREV;"; return 0; }
PREVBASE=''

for m in ${MATRIX//,/ }; do
  case $m in
    55plain) add plain "$e $BIN/p55/rnlua" p55 ;;
    55pgo)   add pgo "$e $BIN/g55/rnlua" g55 ;;
    55int)   add int "$e RUSTYNAIL_JIT=off $BIN/p55/rnlua" i55 ;;
    55nojit) add nj "$e $BIN/n55/rnlua" n55 ;;
    54plain) add p54 "$e $BIN/p54/rnlua" p54 ;;
    54pgo)   add g54 "$e $BIN/g54/rnlua" g54 ;;
    54int)   add i54 "$e RUSTYNAIL_JIT=off $BIN/p54/rnlua" i54 ;;
    54nojit) add n54 "$e $BIN/n54/rnlua" n54 ;;
    *) echo "unknown matrix column: $m" >&2; exit 2 ;;
  esac
done
if [ -z "${BASE:-}" ]; then
  BASE="lj:lj22=/usr/bin/luajit;ljoff:ljoff22=/usr/bin/luajit -joff;$PREVBASE"
  case ",$MATRIX," in *,55*) BASE="${BASE}puc:puc55=$R/55/build-seed0/lua;b1p:b1p-108b935;b1g:b1g-108b935;" ;; esac
  case ",$MATRIX," in *,54*) BASE="${BASE}puc54:puc54=$R/54/build-seed0/lua;" ;; esac
fi
export COLS PAIR BASE OUT ONLY RUNS CPUS
export SPIN_MS=${SPIN_MS:-300} SETS=${SETS:-corpus,corpus2/dev,corpus2/heldout,awfy,programs,jit-trace,lib,apps,real}
[ "$ALLFULL" = 1 ] && export ALL_FULL=1
echo "bench-fast: matrix $MATRIX, cpus $CPUS, out $OUT" >&2
exec systemd-run --user --scope -q -p MemoryMax="${MEMMAX:-12G}" -p MemorySwapMax=0 \
  timeout "${TIMEOUT:-14400}" python3 "$here/scripts/bench_fast.py" run </dev/null
