#!/bin/bash
# Seed corpora of the JIT fuzz targets (jit_diff, and run_source built with --features jit).
# usage: fuzz/make_jit_seeds.sh [rnlua]     (default: target/release/rnlua; writes fuzz/corpus-jit)
#
# jit_diff input = config byte + payload. Config: bit0 generational GC, bit1 GC stress, bits 2-3
# instruction budget (0: 300000, 1: 5000, 2: 977, 3: 61), bit 7 raw source (otherwise the payload
# drives the grammar in fuzz/src/gen.rs and any byte string is a valid description). Bits 5-6 select tier 1.5 and
# add a second config byte (see fuzz/fuzz_targets/jit_diff.rs).
# Raw seeds are tests/jitdiff/gen.lua programs (print bound to the sandbox's `emit`) and small
# bench/programs; grammar seeds are random bytes (coverage-guided mutation does the rest).
# run_source input: config byte (bit 7 = run compiled) + Lua source.
set -e
cd "$(dirname "$0")/.."
L=${1:-target/release/rnlua}
JC=${JIT_CORPUS:-fuzz/corpus-jit}
mkdir -p $JC/jit_diff $JC/run_source
TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT
byte() { printf "\\$(printf '%03o' "$1")"; }
i=0
for cfg in 0 1 2 3 4 8 12 5 9 16 17 18 20 24 28; do
  for k in 1 2 3; do
    i=$((i+1))
    { byte $cfg; head -c $((150 + k * 150)) /dev/urandom; } > $JC/jit_diff/g$(printf %03d $i).bin
  done
done
n=0
for sd in $(seq 1 ${JIT_GEN_SEEDS:-24}); do
  timeout 30 $L tests/jitdiff/gen.lua $sd </dev/null > "$TMP" 2>/dev/null || continue
  [ "$(stat -c %s "$TMP")" -lt 16000 ] || continue
  case $((sd % 3)) in 0) cfg=128;; 1) cfg=132;; *) cfg=137;; esac
  { byte $cfg; echo 'local print = emit'; cat "$TMP"; } > $JC/jit_diff/r$(printf %03d $sd).bin
  n=$((n+1))
done
# tier 1.5 variants (docs/JIT_TIER15.md section 22): config bits 5-6 = T15 mode (1 always, 2 profiled, 3 always+verify), then
# one byte: bits 0-2 exit storm index (off 1 2 7 61), bit 3 +shadow, bits 4-7 +randra seed
for sd in $(seq 1 ${JIT_T15_SEEDS:-24}); do
  timeout 30 $L tests/jitdiff/gen.lua $sd </dev/null > "$TMP" 2>/dev/null || continue
  [ "$(stat -c %s "$TMP")" -lt 16000 ] || continue
  sel=$((1 + sd % 3)); tb=$(( (sd % 5) | (((sd / 2) % 2) << 3) | (((sd % 15) + 1) << 4) ))
  { byte $((128 | sel * 32)); byte $tb; echo 'local print = emit'; cat "$TMP"; } > $JC/jit_diff/t$(printf %03d $sd).bin
done
for f in bench/programs/*.lua tests/lua/gen/*.lua tests/vm/*.lua; do
  [ -f "$f" ] || continue
  [ "$(stat -c %s "$f")" -lt 3500 ] || continue
  n=$((n+1)); b=$(printf "%04d" $n)
  { byte 128; cat "$f"; } > $JC/run_source/j$b.bin    # compiled, incremental GC
  { byte 131; cat "$f"; } > $JC/run_source/j${b}s.bin  # compiled, generational + stress
done
# bench programs (mostly > 3.5 KB or heavy) as jit_diff raw seeds when small enough
for f in bench/programs/*.lua; do
  [ "$(stat -c %s "$f")" -lt 8000 ] || continue
  [ "$(basename "$f")" != load_big.lua ] || continue   # 25 compiles of a 400 KB chunk: 30 s per run
  { byte 128; echo 'local print = emit'; cat "$f"; } > $JC/jit_diff/b$(basename "$f" .lua).bin
done
echo "jit seeds: jit_diff=$(ls $JC/jit_diff | wc -l) run_source=$(ls $JC/run_source | wc -l)"
