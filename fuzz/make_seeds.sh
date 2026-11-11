#!/bin/bash
# Seed corpora for the fuzz targets from the repository's Lua programs.
# usage: fuzz/make_seeds.sh   (needs target/debug/rnlua built)
#        LUA_VERSION=5.4 fuzz/make_seeds.sh   (needs target-54/debug/rnlua; writes fuzz/corpus54)
set -e
cd "$(dirname "$0")/.."
if [ "${LUA_VERSION:-5.5}" = 5.4 ]; then
  L=target-54/debug/rnlua; OUT=fuzz/corpus54; GEN=tests/lua/gen54
else
  L=target/debug/rnlua; OUT=fuzz/corpus; GEN=tests/lua/gen
fi
DUMP=$(mktemp --suffix=.lua)
trap 'rm -f "$DUMP"' EXIT
mkdir -p $OUT/lex_parse $OUT/run_source $OUT/undump $OUT/string_lib $OUT/table_ops $OUT/capi_calls $OUT/buffer_ops
n=0
for f in $GEN/*.lua tests/vm/*.lua tests/lua/*.lua; do
  [ -f "$f" ] || continue
  sz=$(stat -c %s "$f"); [ "$sz" -lt 3500 ] || continue
  n=$((n+1)); b=$(printf "%04d" $n)
  cp "$f" $OUT/lex_parse/$b.lua
  { printf '\x00'; cat "$f"; } > $OUT/run_source/$b.bin            # config byte 0: incremental, no stress
  { printf '\x01'; cat "$f"; } > $OUT/run_source/${b}g.bin          # generational
done
# binary chunks (without the leading ESC: the target prepends it)
cat > "$DUMP" <<'LUA'
local src, out, strip = ...
local f = assert(loadfile(src))
local d = string.dump(f, strip == "1")
local o = assert(io.open(out, "wb")); o:write(d:sub(2)); o:close()
LUA
m=0
for f in $GEN/*.lua tests/vm/co_0*.lua tests/vm/meta_0*.lua; do
  m=$((m+1)); [ $m -le 120 ] || break
  b=$(printf "%04d" $m)
  $L "$DUMP" "$f" $OUT/undump/$b.bin 0 2>/dev/null || true
  $L "$DUMP" "$f" $OUT/undump/${b}s.bin 1 2>/dev/null || true
done
# a few hand-made seeds for the library targets
printf '\x00\x00\x00%%5.2f %%d %%s %%q' > $OUT/string_lib/fmt1
# string.buffer: put/get/encode/decode op streams (config byte, then op, arg, bytes...)
printf '\x00\x00\x05hello\x08\x04abcd\x01\x03\x09\x02xy\x06\x03\x0c\x01\x0d\x00' > $OUT/buffer_ops/ops1
printf '\x01\x01\x00i4 >I2 z s1 d' > $OUT/string_lib/pack1
printf '\x00\x04\x03(%%a+)%%s*(%%d+)' > $OUT/string_lib/pat1
head -c 400 /dev/urandom > $OUT/table_ops/rand1
head -c 600 /dev/urandom > $OUT/capi_calls/rand1
head -c 600 /dev/urandom > $OUT/capi_calls/rand2
[ "${LUA_VERSION:-5.5}" = 5.4 ] || fuzz/make_jit_seeds.sh "$L"   # JIT targets -> fuzz/corpus-jit
echo "seeds: lex_parse=$(ls $OUT/lex_parse | wc -l) run_source=$(ls $OUT/run_source | wc -l) undump=$(ls $OUT/undump | wc -l)"
