#!/bin/bash
# Seed corpora of the J5 trace fuzz targets (build them with `--features trace`).
# usage: fuzz/make_trace_seeds.sh [rnlua]     (default: target/release/rnlua; writes fuzz/corpus-trace)
#
# trace_ir   input = 8 bytes seed + 3 config bytes (see fuzz/src/trace_harness_ext.rs): random bytes, with the
#            config bytes enumerating every backend / pass mask so that each stage is seeded.
# jit_diff   input = 0xFF, trace byte, config byte, payload (see the header of fuzz/fuzz_targets/jit_diff.rs). Raw
#            seeds are tests/jitdiff/gen.lua programs of the trace template set (arg 4 = tr) with print bound to the
#            sandbox's `emit`; grammar seeds are random bytes behind the same prefix.
# run_source input = config byte (bit 7 compiled, bits 2-3 trace mode, bits 4-6 option variant) + Lua source.
set -e
cd "$(dirname "$0")/.."
L=${1:-target/release/rnlua}
TC=${TRACE_CORPUS:-fuzz/corpus-trace}
mkdir -p $TC/trace_ir $TC/jit_diff $TC/run_source
TMP=$(mktemp)
trap 'rm -f "$TMP"' EXIT
byte() { printf "\\$(printf '%03o' "$1")"; }
# trace_ir: seed + (size/loops/vb, pass mask + backend, ra) for every backend (bits 5-7 of c2) and pass combination
i=0
for be in 0 1 2 3 4 5 6 7; do
  for pm in 0 3 7 15 23 31; do
    for ld in 0 32; do
      i=$((i+1))
      { head -c 8 /dev/urandom; byte $(( (6 + (i % 20)) | ld | ((i % 2) << 6) )); byte $(( pm | (be << 5) )); byte $((i % 4)); } > $TC/trace_ir/t$(printf %03d $i).bin
    done
  done
done
# jit_diff: raw gen.lua programs behind the trace prefix, every trace mode / option variant
n=0
for sd in $(seq 1 ${TRACE_GEN_SEEDS:-60}); do
  timeout 30 $L tests/jitdiff/gen.lua $sd "" "" tr </dev/null > "$TMP" 2>/dev/null || continue
  [ "$(stat -c %s "$TMP")" -lt 16000 ] || continue
  t=$(( (sd % 4) | (((sd / 4) % 8) << 2) | (((sd / 32) % 2) << 5) ))
  { byte 255; byte $t; byte $((128 + sd % 4 + (sd % 3 == 0 ? 32 : 0))); echo 'local print = emit'; cat "$TMP"; } > $TC/jit_diff/r$(printf %03d $sd).bin
  n=$((n+1))
done
for k in $(seq 1 40); do
  { byte 255; byte $(( (k % 4) | ((k % 8) << 2) )); byte $(( (k % 3) | ((k % 2) << 4) )); head -c $((150 + k * 40)) /dev/urandom; } > $TC/jit_diff/g$(printf %03d $k).bin
done
# run_source: gen.lua programs (print silenced: the sandbox has none) in every trace mode / variant, compiled
m=0
for sd in $(seq 1 ${TRACE_GEN_SEEDS:-60}); do
  timeout 30 $L tests/jitdiff/gen.lua $sd "" "$((sd % 5 + 1))" tr </dev/null > "$TMP" 2>/dev/null || continue
  [ "$(stat -c %s "$TMP")" -lt 3900 ] || continue
  m=$((m+1))
  { byte $(( 128 | ((1 + sd % 3) << 2) | (((sd / 3) % 8) << 4) | (sd % 4 & 3) )); echo 'local print = function() end'; cat "$TMP"; } > $TC/run_source/s$(printf %03d $sd).bin
done
# tiny hand-written loops (the shapes of bench/jit-trace): sum, recurrence, intrinsics, nested, aliasing, type flip
w=0
while IFS= read -r line; do
  w=$((w+1))
  for mode in 1 2 3; do
    { byte $(( 128 | (mode << 2) | (((w + mode) % 8) << 4) )); echo "$line"; } > $TC/run_source/h$(printf %02d $w)_$mode.bin
  done
done <<'EOF'
local t = {} for i = 1, 200 do t[i] = i end local s = 0 for i = 1, 200 do s = s + t[i] * 2 end return s
local t = {1.5} for i = 2, 300 do t[i] = t[i-1] * 1.01 + 0.5 end return t[300]
local s = 0 for i = 1, 300 do s = s + math.sqrt(i) + math.abs(-i) + math.floor(i / 3) + math.max(i, 7) end return s
local s = 0 for i = 1, 40 do for j = 1, i % 7 do s = s + i * j end end return s
local a = {1, 2, 3} local b = a for i = 2, 100 do a[i] = b[i-1] + 1 end return a[100]
local s = 0 for i = 1, 200 do if i == 100 then s = s + 0.5 end s = s + i end return s
local t = {} for i = 1, 100 do t[i] = i end t[50] = "x" local s = 0 for i = 1, 100 do s = s + t[i] end return s
local acc = 0 local function f(x) return x * 2 + 1 end for i = 1, 300 do acc = acc + f(i) end return acc
local up = 0 local function inc() up = up + 1 end for i = 1, 300 do inc() end return up
local x = 0.0 for v = 0.0, 100, 0.25 do x = x + v * v end return x
local i, s = 1, 0 while i <= 500 do s = s + i % 7; i = i + 2 end return s
local t = {} for i = 1, 64 do t[i] = (i * 53) % 97 end for p = 1, 8 do for j = 1, 63 do if t[j] > t[j+1] then t[j], t[j+1] = t[j+1], t[j] end end end return t[1]
local s = 0 for i = 1, 300 do local v = {x = i, y = i + 1} s = s + v.x * v.y end return s
local s = 0 for i = 1, 300 do local v = {i, i + 1} if i % 97 == 0 then s = s + #v end s = s + v[1] end return s
local keep for i = 1, 300 do local v = {a = i, b = 2} if i == 250 then keep = v end end return keep.a
local n = 0 for i = 1, 400 do local v = {x = i} setmetatable({}, {__gc = function() n = n + 1 end}) if i % 50 == 0 then collectgarbage() end end return n, collectgarbage("count") > 0
local function g(p, n) if n == 0 then return p.x end return g({x = p.x + 1, y = p.y}, n - 1) end local s = 0 for i = 1, 100 do s = s + g({x = i, y = 1}, 5) end return s
local ok = 0 for i = 1, 300 do local v = {x = i} local r = pcall(function() return v.x + (i == 200 and nil or 1) end) if r then ok = ok + 1 end end return ok
local w = setmetatable({}, {__mode = "v"}) for i = 1, 300 do local v = {x = i, y = 2} if i % 30 == 0 then w[i] = v end if i % 100 == 0 then collectgarbage("step", 1) end end return #w
EOF
# regression inputs of fuzz findings (fuzz/regress/<target>-*.bin)
for f in fuzz/regress/jit_diff-*.bin; do [ -f "$f" ] && cp "$f" $TC/jit_diff/; done
for f in fuzz/regress/run_source-*.bin; do [ -f "$f" ] && cp "$f" $TC/run_source/; done
echo "trace seeds: trace_ir=$(ls $TC/trace_ir | wc -l) jit_diff=$(ls $TC/jit_diff | wc -l) run_source=$(ls $TC/run_source | wc -l)"
