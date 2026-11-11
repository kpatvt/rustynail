#!/bin/bash
# Per-merge benchmark tracking: runs a fixed small set of programs (min cycles:u of RUNS pinned, interleaved runs)
# and appends ONE row to docs/bench-history.tsv. Columns of the measurement: ref = PUC 5.5 (constant-seed build),
# cur = BIN (traces on, the process default), j4 = BIN with RUSTYNAIL_JIT_TRACE=off, dup = BIN again (duplicate
# baseline: cur/dup is the noise of this very row; trust a change of cur only if it is larger than that).
#
#   BIN=/path/to/rnlua scripts/bench-track.sh [label]
#
# Env: BIN (default target/release/rnlua, must be built with --features jit), REF (default ref/55/build-seed0/lua
# of the main checkout), RUNS=7, CPU=4 (not 0-3), LOADMAX=2.0 (1-minute load average above which the row is flagged
# NOISY; FORCE=0 refuses to record it), PROGS (comma list), HIST (default docs/bench-history.tsv),
# OUT (json of the run, default a temp file).
# Runs under a memory cap and timeout as CLAUDE.md requires; stdin is /dev/null.
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
bin="${BIN:-$here/target/release/rnlua}"
common="$(git -C "$here" rev-parse --git-common-dir 2>/dev/null || true)"
ref="${REF:-$(cd "$(dirname "$common")" 2>/dev/null && pwd)/ref/55/build-seed0/lua}"
[ -x "$bin" ] || { echo "BIN $bin not found (build: cargo build --release -p rn-cli --features jit)"; exit 2; }
[ -x "$ref" ] || { echo "REF $ref not found"; exit 2; }
export RUNS="${RUNS:-7}" CPU="${CPU:-4}"
export ONLY="${PROGS:-fib,nbody,fasta,table_hash,string_ops,json,binary_trees,fannkuch,oop_methods,pcall_error,load_big,coroutines}"
loadmax="${LOADMAX:-2.0}"
load1="$(cut -d' ' -f1 /proc/loadavg)"
noisy=0
if awk -v l="$load1" -v m="$loadmax" 'BEGIN{exit !(l>m)}'; then noisy=1; fi
if [ "$noisy" = 1 ] && [ "${FORCE:-1}" = 0 ]; then
  echo "load average $load1 > $loadmax: machine not quiet, refusing (FORCE=1 records the row flagged NOISY)"; exit 3
fi
out="${OUT:-$(mktemp /tmp/bench-track.XXXXXX.json)}"
export OUT="$out"
hist="${HIST:-$here/docs/bench-history.tsv}"
label="${1:-}"
rev="$(git -C "$here" rev-parse --short HEAD)"
dirty=""; [ -n "$(git -C "$here" status --porcelain --untracked-files=no 2>/dev/null)" ] && dirty="+dirty"
export RUSTYNAIL_SEED=0
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 \
  timeout "${TIMEOUT:-1800}" python3 "$here/scripts/bench_cycles.py" \
  "ref=env RUSTYNAIL_SEED=0 $ref" "cur=env RUSTYNAIL_SEED=0 $bin" "j4=env RUSTYNAIL_SEED=0 RUSTYNAIL_JIT_TRACE=off $bin" \
  "dup=env RUSTYNAIL_SEED=0 $bin" </dev/null >/dev/null 2>"$out.err" || { cat "$out.err"; exit 1; }
python3 "$here/scripts/bench_track_row.py" "$out" "$hist" "$rev$dirty" "$load1" "$noisy" "$label"
rm -f "$out" "$out.err"
