#!/bin/bash
# fresh LuaJIT baseline (LuaJIT 2.1.1795119671 upgraded 2026-11-23): measured as subject columns, then imported as lj22 / ljoff22
cd "$(dirname "$0")/../../.."
J=bench/results/lj22
CPUS=4 RUNS=9 ALL_FULL=1 FORCE_STALE=1 \
 COLS='ljn=/usr/bin/luajit;ljoffn=/usr/bin/luajit -joff' BASE='' PAIR='' OUT=$PWD/$J/lj_fresh.json \
 systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 14000 python3 scripts/bench_fast.py run </dev/null > $J/run.log 2>&1
echo "run exit $?" >> $J/run.log
