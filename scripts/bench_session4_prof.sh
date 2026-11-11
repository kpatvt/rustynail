#!/bin/bash
# usage: prof.sh <prog relative path> ; perf record on new vs previous plain binary, top symbols
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"; cd "$ROOT"
export RUSTYNAIL_SEED=0
B=${BENCH_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/bench1110}
for h in 4ce16b7a 53133e8d; do
  echo "== $h $1"
  taskset -c 4 perf stat -e cycles:u,instructions:u,branch-misses:u,L1-dcache-load-misses:u -- $B/$h/tplain/release/rnlua $1 </dev/null 2>&1 >/dev/null | grep -E "cycles|instr|branch|L1"
  taskset -c 4 perf record -q -o $B/perf-$h.data -e cycles:u -- $B/$h/tplain/release/rnlua $1 </dev/null >/dev/null 2>&1
  perf report -i $B/perf-$h.data --no-children --stdio 2>/dev/null | grep -v "^#" | head -12
done
