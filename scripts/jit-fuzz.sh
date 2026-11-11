#!/usr/bin/env bash
# Memory-capped runner for the JIT fuzz targets (docs/JIT_TESTING.md, "Fuzzing").
#   scripts/jit-fuzz.sh build                      build the targets with --features $FUZZ_FEATURES (default jit; `trace` for the J5
#                                                  targets trace_ir + the trace bits of jit_diff / run_source, corpus: CORPUS_DIR=fuzz/corpus-trace after fuzz/make_trace_seeds.sh)
#   scripts/jit-fuzz.sh run <target> [secs] [extra libFuzzer args]   (target: jit_diff | run_source | trace_ir)
#   scripts/jit-fuzz.sh repro <target> <file>      run one input (JIT_DIFF_DUMP=1 prints the program;
#                                                  JIT_DIFF_OUT=<dir> writes src.lua interp.log jit.log)
#   scripts/jit-fuzz.sh lua <file.lua>             run a Lua file (emit() for output) interpreter vs JIT [OUT=dir]
# Env: FUZZ_TARGET_DIR (cargo target dir, default $TMPDIR/jitfuzz-target), CORPUS_DIR (default fuzz/corpus-jit)
set -eu
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TD=${FUZZ_TARGET_DIR:-${TMPDIR:-/tmp}/jitfuzz-target}
CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
BIN="$TD/x86_64-unknown-linux-gnu/release"
cmd=${1:?usage}; shift
cd "$HERE"
case $cmd in
  build)
    CARGO_TARGET_DIR=$TD $CAP timeout 1800 cargo +nightly fuzz build --features ${FUZZ_FEATURES:-jit} </dev/null ;;
  run)
    t=$1; secs=${2:-900}; shift; shift || true
    corp=${CORPUS_DIR:-fuzz/corpus-jit}/$t
    mkdir -p "$corp" fuzz/artifacts/$t
    $CAP timeout $((secs + 120)) "$BIN/$t" "$corp" -max_total_time="$secs" -rss_limit_mb=2500 -timeout=30 \
      -artifact_prefix=fuzz/artifacts/$t/ -print_final_stats=1 "$@" </dev/null ;;
  repro)
    $CAP timeout 120 "$BIN/$1" "$2" </dev/null ;;
  lua)  # run a Lua file (uses emit(...) for output) interpreter vs JIT; logs in $OUT (default .)
    f=$(mktemp); trap 'rm -f $f' EXIT
    { printf '\200local print = emit\n'; cat "$1"; } > $f
    JIT_DIFF_OUT=${OUT:-.} $CAP timeout 120 "$BIN/jit_diff" $f </dev/null ;;
  *) echo "unknown command $cmd" >&2; exit 2 ;;
esac
