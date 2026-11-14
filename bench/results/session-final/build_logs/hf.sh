#!/bin/bash
# wall-clock startup with hyperfine (-N: no shell), pinned to CPU 4, 200 runs.
B=<cache>/bin; R=<repo>/ref
OUT=<worktree>/bench/results/session-final
export RUSTYNAIL_SEED=0
echo "" > $OUT/empty.lua
taskset -c 4 hyperfine -N --warmup 20 --runs 200 --export-json $OUT/startup_hyperfine.json \
  -n rn55plain "$B/p55/rnlua $OUT/empty.lua" -n rn55pgo "$B/g55/rnlua $OUT/empty.lua" \
  -n rn55nojit-build "$B/n55/rnlua $OUT/empty.lua" \
  -n rn54plain "$B/p54/rnlua $OUT/empty.lua" -n rn54pgo "$B/g54/rnlua $OUT/empty.lua" \
  -n 9a9b8ed-plain "$B/b9p/rnlua $OUT/empty.lua" -n 9a9b8ed-pgo "$B/b9g/rnlua $OUT/empty.lua" \
  -n 8a18eb3-plain "$B/b8p/rnlua $OUT/empty.lua" -n 8a18eb3-pgo "$B/b8g/rnlua $OUT/empty.lua" \
  -n PUC55 "$R/55/build-seed0/lua $OUT/empty.lua" -n PUC54 "$R/54/build-seed0/lua $OUT/empty.lua" -n LuaJIT "/usr/bin/luajit $OUT/empty.lua"
RUSTYNAIL_JIT=off taskset -c 4 hyperfine -N --warmup 20 --runs 200 --export-json $OUT/startup_hyperfine_jitoff.json -n rn55plain-JIT=off "$B/p55/rnlua $OUT/empty.lua"
