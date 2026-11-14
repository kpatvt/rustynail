#!/bin/bash
# startup anomaly check: alternate JIT on / JIT off hyperfine runs (3 rounds), pinned to CPU 4
B=<cache>/bin
OUT=<worktree>/bench/results/session-final
export RUSTYNAIL_SEED=0
for r in 1 2 3; do
  taskset -c 4 hyperfine -N --warmup 20 --runs 200 --export-json $OUT/startup_hf_recheck_on_$r.json -n rn55plain "$B/p55/rnlua $OUT/empty.lua" 2>&1 | grep -E "Time|Range"
  RUSTYNAIL_JIT=off taskset -c 4 hyperfine -N --warmup 20 --runs 200 --export-json $OUT/startup_hf_recheck_off_$r.json -n rn55plain-JIT=off "$B/p55/rnlua $OUT/empty.lua" 2>&1 | grep -E "Time|Range"
done
taskset -c 4 hyperfine -N --warmup 20 --runs 200 --export-json $OUT/startup_hf_recheck_puc.json -n PUC55 "<repo>/ref/55/build-seed0/lua $OUT/empty.lua" 2>&1 | grep -E "Time|Range"
