#!/bin/bash
# scripts/ext-diff.sh [--update] [--luajit PATH] [--rnlua PATH] [test.lua ...]
#
# Differential runner for the LuaJIT-compatible extensions (docs/EXTENSIONS.md): every tests/ext/NAME.lua is run under
# LuaJIT (the oracle, default /usr/bin/luajit) and under rnlua; both outputs must be identical and must equal the
# checked-in NAME.expected. Tests named rn_*.lua exercise rustynail-only behaviour (integer subtype, JIT control, ...)
# and are compared with their .expected only. `--update` rewrites NAME.expected from LuaJIT (rn_* tests: from rnlua).
# Every run is memory-capped and has stdin on /dev/null (CLAUDE.md). The environment is passed through, so the JIT /
# GC modes can be varied: RUSTYNAIL_JIT=off|on|always, RUSTYNAIL_JIT_TRACE=always, RUSTYNAIL_GC_STRESS=1|2, ...
set -u
root=$(cd "$(dirname "$0")/.." && pwd)
luajit=/usr/bin/luajit
rnlua=$root/target/release/rnlua
update=0
tests=()
while [ $# -gt 0 ]; do
  case "$1" in
    --update) update=1 ;;
    --luajit) luajit=$2; shift ;;
    --rnlua) rnlua=$2; shift ;;
    *) tests+=("$1") ;;
  esac
  shift
done
cd "$root/tests/ext" || exit 2
if [ ${#tests[@]} -eq 0 ]; then
  for f in *.lua; do case "$f" in _*) ;; *) tests+=("$f") ;; esac; done
fi
cap() { systemd-run --user --scope -q -p MemoryMax=6G -p MemorySwapMax=0 timeout "${EXT_TIMEOUT:-300}" "$@" </dev/null; }
norm() { sed -E 's/0x[0-9a-f]{6,}/0xADDR/g'; }
fail=0; pass=0
for t in "${tests[@]}"; do
  t=$(basename "$t"); name=${t%.lua}; exp=$name.expected
  ours=$(cap "$rnlua" "$t" 2>&1 | norm)
  case "$name" in
    rn_*) ref=$ours; oracle=0 ;;
    *) ref=$(cap "$luajit" "$t" 2>&1 | norm); oracle=1 ;;
  esac
  if [ $update -eq 1 ]; then printf '%s\n' "$ref" > "$exp"; fi
  ok=1
  if [ $oracle -eq 1 ] && [ "$ours" != "$ref" ]; then
    ok=0; echo "FAIL $t: rnlua differs from LuaJIT"; diff <(printf '%s\n' "$ref") <(printf '%s\n' "$ours") | head -${EXT_DIFF_LINES:-20}
  fi
  if [ ! -f "$exp" ]; then ok=0; echo "FAIL $t: no $exp (run with --update)"
  elif [ "$ours" != "$(cat "$exp")" ]; then
    ok=0; echo "FAIL $t: rnlua differs from $exp"; diff "$exp" <(printf '%s\n' "$ours") | head -${EXT_DIFF_LINES:-20}
  fi
  if [ $ok -eq 1 ]; then pass=$((pass + 1)); else fail=$((fail + 1)); fi
done
echo "ext-diff: $pass passed, $fail failed"
[ $fail -eq 0 ]
