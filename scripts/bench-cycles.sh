#!/bin/bash
# Interleaved, pinned, perf-counter benchmark (JIT plan J0). Usage:
#   scripts/bench-cycles.sh ref=ref/55/build-seed0/lua ours="env RUSTYNAIL_SEED=0 target/release/rnlua" lj=luajit
# Each argument is name=command (the first is the baseline; commands may carry arguments).
# Env: CPU=4 (must not be 0-3 for real measurements), RUNS=9, ONLY=prog,prog, OUT=file.json,
#      LJ_BINS=lj (binaries that use bench/luajit/<prog>.lua variants), EXCLUDE=lj:json+load_big.
# String-hash seed: our rnlua honours RUSTYNAIL_SEED; the reference needs a build with a constant
# luai_makeseed (ref/55/build-seed0); LuaJIT hashing is not seeded per run. Wrap in `env RUSTYNAIL_SEED=0 ...`.
# F9: a rnlua built with the `jit` feature runs traces by default (RUSTYNAIL_JIT_TRACE unset = on): keep JIT columns
# explicit, e.g. j4="env RUSTYNAIL_JIT_TRACE=off target/jit/release/rnlua" tr="env RUSTYNAIL_JIT_TRACE=on ...".
# Runs under a memory cap and timeout as required by CLAUDE.md.
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
export CPU="${CPU:-4}" RUNS="${RUNS:-9}"
exec systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 \
  timeout "${TIMEOUT:-3000}" python3 "$here/scripts/bench_cycles.py" "$@" </dev/null
