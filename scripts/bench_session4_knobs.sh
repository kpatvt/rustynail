#!/bin/bash
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"; cd "$ROOT"
export RUSTYNAIL_SEED=0
B=${BENCH_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/bench1110}
for p in bench/lib/tonumber_num.lua "bench/programs/fib.lua"; do
for k in "" pollr15=0 chainre=0 ncallee=0; do
  for h in 4ce16b7a; do
    echo -n "$p [$k] "
    RUSTYNAIL_JIT_TRACE_LIMITS="$k" taskset -c 4 perf stat -x, -e cycles:u,instructions:u -- $B/$h/tplain/release/rnlua $p </dev/null 2>&1 >/dev/null | cut -d, -f1 | tr '\n' ' '
    echo
  done
done
echo -n "$p [53133e8d] "
taskset -c 4 perf stat -x, -e cycles:u,instructions:u -- $B/53133e8d/tplain/release/rnlua $p </dev/null 2>&1 >/dev/null | cut -d, -f1 | tr '\n' ' '
echo
done
