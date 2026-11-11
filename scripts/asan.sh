#!/bin/bash
# V8: AddressSanitizer (+ LeakSanitizer) run of the whole system.
#   1. builds rnlua and rn-capi with `-Zsanitizer=address` (nightly + build-std) and the C side
#      (shim, C tests) with -fsanitize=address;
#   2. runs the official test suite through the ASan rnlua;
#   3. runs the C API differential sections, the vendored lua.c (official suite) and the sample C
#      module against the ASan library.
# usage: scripts/asan.sh [target-dir]      (default /tmp/rustynail-asan; needs nightly + rust-src)
#        LUA_VERSION=5.4 scripts/asan.sh [target-dir]   (the 5.4 build, default dir /tmp/rustynail-asan-54)
#        RUSTYNAIL_GC_STRESS=1|2 [RUSTYNAIL_GC_MODE=inc] scripts/asan.sh ...   (under GC stress)
#        ASAN_FEATURES=trace RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always scripts/asan.sh ...
#            (F6: rnlua with the J5 trace compiler (cargo feature `trace`, or `jit`); the run-time variables are inherited by
#             the official-suite runs of rnlua; lua.c + librn_capi ignore them)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
LV=${LUA_VERSION:-5.5}
if [ "$LV" = 5.4 ]; then CFG="--cfg rustynail_lua54"; DEF=/tmp/rustynail-asan-54; else CFG=""; DEF=/tmp/rustynail-asan; fi
TGT=${1:-$DEF}
A=$TGT/x86_64-unknown-linux-gnu/debug
export CARGO_TARGET_DIR=$TGT
export RUSTFLAGS="-Zsanitizer=address -Cdebuginfo=1 --cfg rustynail_lsan $CFG"
export CFLAGS="-fsanitize=address -fno-omit-frame-pointer -g"
export ASAN_OPTIONS=detect_leaks=1:halt_on_error=1
export RUSTYNAIL_SYSTEM_MALLOC=1 # plain malloc: the pool allocator would hide use-after-free
cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu -p rn-cli -p rn-capi --no-default-features ${ASAN_FEATURES:+--features rn-cli/$ASAN_FEATURES}
V=vendor/lua-$LV/src
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
CC="gcc -O1 -g -fsanitize=address -fno-omit-frame-pointer -w -DLUA_USE_POSIX -I$V"
$CC -o "$W/lua_c" $V/lua.c "$A/librn_capi.a" -lm -ldl -lpthread
$CC -o "$W/capi_diff" tests/c/capi_diff.c "$A/librn_capi.a" -lm -ldl -lpthread
gcc -shared -fPIC -O1 -w -I$V -o "$W/samplemod.so" tests/c/samplemod.c

fail=0
# Under RUSTYNAIL_GC_STRESS gc.lua's assertions about collection progress do not hold (a collection
# at every safepoint clears weak entries / finishes cycles earlier than the test expects): skip it.
SKIP='^(all|heavy)\.lua$'
if [ -n "${RUSTYNAIL_GC_STRESS:-}" ] && [ "${RUSTYNAIL_GC_STRESS}" != 0 ]; then SKIP='^(all|heavy|gc)\.lua$'; fi
run_suite() { # binary label
  for f in $(ls vendor/lua-tests-$LV/*.lua | xargs -n1 basename | grep -vE "$SKIP"); do
    if ! (cd vendor/lua-tests-$LV && timeout 600 "$1" -e "_port=true;_soft=true" "$f" </dev/null >/dev/null 2>"$W/err"); then
      echo "FAIL [$2] $f: $(head -3 "$W/err")"; fail=1
    fi
  done
}
echo "== official suite through the ASan rnlua"; run_suite "$A/rnlua" rnlua
echo "== official suite through lua.c + ASan librn_capi"; run_suite "$W/lua_c" lua.c
echo "== C API sections"
for t in stack types arith tables calls load coroutines auxcheck auxmisc auxresults userdata debug gc misc alloc iostream; do
  if ! timeout 300 "$W/capi_diff" "$t" </dev/null >/dev/null 2>"$W/err"; then echo "FAIL [capi] $t: $(head -3 "$W/err")"; fail=1; fi
done
echo "== C module"
if ! LUA_CPATH="$W/?.so" "$A/rnlua" tests/c/samplemod.lua >/dev/null 2>"$W/err"; then echo "FAIL [samplemod] $(head -3 "$W/err")"; fail=1; fi
[ $fail = 0 ] && echo "ASAN OK" || { echo "ASAN FAILURES"; exit 1; }
