#!/bin/bash
# layout hypothesis for type_late: prepend k extra hot loops (each leaves a trace of different size) before the program body
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)"; cd "$ROOT"
for k in 0 1 2 3 4; do
  { for i in $(seq 1 $k); do
      echo "local function pre$i(n) local s = 0 for j = 1, n do s = s + j * $i end return s end"
      echo "for r = 1, 300 do pre$i(1000) end"
    done
    cat bench/jit-trace/type_late.lua; } > ${TMPDIR:-/tmp}/p9/shift$k.lua
  for f in type_late_stable; do
    { for i in $(seq 1 $k); do
      echo "local function pre$i(n) local s = 0 for j = 1, n do s = s + j * $i end return s end"
      echo "for r = 1, 300 do pre$i(1000) end"
    done
    cat bench/jit-trace/$f.lua; } > ${TMPDIR:-/tmp}/p9/shifts$k.lua
  done
  for v in shift shifts; do
    RUSTYNAIL_SEED=0 RUSTYNAIL_JIT=on RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=on perf stat -x, -e cycles:u -o ${TMPDIR:-/tmp}/p9/pt.out taskset -c 4 ${TMPDIR:-/tmp}/p9/bin/luaj5 ${TMPDIR:-/tmp}/p9/$v$k.lua </dev/null >/dev/null
    echo "$v k=$k cycles $(cut -d, -f1 ${TMPDIR:-/tmp}/p9/pt.out | grep -E '^[0-9]+$')"
  done
done
