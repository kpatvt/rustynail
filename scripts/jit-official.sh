#!/bin/bash
# Runs the official Lua test suite (vendor/lua-tests-<ver>) through a `--features jit` rnlua with the given
# JIT environment, without touching tests/official-status*.toml (unlike `cargo xtask official`).
#
#   scripts/jit-official.sh <rnlua> [VAR=value ...]       e.g.  RUSTYNAIL_JIT=always
#   LUA_VERSION=5.4 scripts/jit-official.sh <rnlua> RUSTYNAIL_JIT=always     (a 5.4 binary)
# Exit status 0 iff every file (except all.lua / heavy.lua, as in scripts/asan.sh) passes.
set -uo pipefail
cd "$(dirname "$0")/.."
BIN=$(realpath "${1:?rnlua binary}"); shift
LV=${LUA_VERSION:-5.5}
CAP=(systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0)
"${CAP[@]}" true </dev/null 2>/dev/null || CAP=()
fail=0
for f in $(ls vendor/lua-tests-$LV/*.lua | xargs -n1 basename | grep -vE '^(all|heavy)\.lua$'); do
  if ! (cd vendor/lua-tests-$LV && env "$@" "${CAP[@]}" timeout 900 "$BIN" -e "_port=true;_soft=true" "$f" </dev/null >/dev/null 2>/tmp/jit-official.$$.err); then
    echo "FAIL $f: $(head -3 /tmp/jit-official.$$.err)"; fail=1
  fi
done
rm -f /tmp/jit-official.$$.err
[ $fail = 0 ] && echo "JIT-OFFICIAL OK ($*)" || { echo "JIT-OFFICIAL FAILED ($*)"; exit 1; }
