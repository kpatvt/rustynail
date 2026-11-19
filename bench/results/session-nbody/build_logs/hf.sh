#!/bin/bash
# wall-clock startup with hyperfine (-N: no shell), pinned to CPU 4, 200 runs. Then the JIT on/off alternating recheck.
B=$HOME/.cache/rnnb/bin; R=/home/themadmin/Projects/rustynail/ref
OUT=/home/themadmin/Projects/rustynail/.claude/worktrees/agent-aad64bca5daa7f0b7/bench/results/session-nbody
export RUSTYNAIL_SEED=0
echo "" > $OUT/empty.lua
taskset -c 4 hyperfine -N --warmup 20 --runs 200 --export-json $OUT/startup_hyperfine.json \
  -n rn55plain "$B/p55/rnlua $OUT/empty.lua" -n rn55pgo "$B/g55/rnlua $OUT/empty.lua" \
  -n rn55nojit-build "$B/n55/rnlua $OUT/empty.lua" \
  -n rn54plain "$B/p54/rnlua $OUT/empty.lua" -n rn54pgo "$B/g54/rnlua $OUT/empty.lua" \
  -n 108b935-plain "$B/b1p/rnlua $OUT/empty.lua" -n 108b935-pgo "$B/b1g/rnlua $OUT/empty.lua" \
  -n PUC55 "$R/55/build-seed0/lua $OUT/empty.lua" -n PUC54 "$R/54/build-seed0/lua $OUT/empty.lua" -n LuaJIT "/usr/bin/luajit $OUT/empty.lua"
for r in 1 2 3; do
  RUSTYNAIL_JIT=off taskset -c 4 hyperfine -N --warmup 20 --runs 200 --export-json $OUT/startup_hf_jitoff_$r.json -n rn55plain-JIT=off "$B/p55/rnlua $OUT/empty.lua" 2>&1 | grep -E "Time|Range"
  taskset -c 4 hyperfine -N --warmup 20 --runs 200 --export-json $OUT/startup_hf_jiton_$r.json -n rn55plain "$B/p55/rnlua $OUT/empty.lua" 2>&1 | grep -E "Time|Range"
done
