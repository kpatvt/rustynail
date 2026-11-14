#!/bin/bash
OUT=<worktree>/bench/results/session-bm5
B=<cache>/rnbm5/bin
export RUSTYNAIL_SEED=0
echo "jitoff rerun $(date)"
RUSTYNAIL_JIT=off taskset -c 4 hyperfine -N --warmup 20 --runs 200 --export-json $OUT/startup_hyperfine_jitoff.json -n rn55plain-JIT=off "$B/p55/rnlua $OUT/empty.lua" 2>&1 | grep -E "Time|Range"
