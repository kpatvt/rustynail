#!/bin/bash
# BUNDLE-K L1: observer programs of tests/lazyci/*.lua against the reference interpreter (output + exit status), for lazyci off / on in several
# trace modes. usage: scripts/lazyci-diff.sh [rnlua] [ref lua]
# Every run is memory capped and timed (CLAUDE.md); stdin is /dev/null.
BIN=${1:-target/release/rnlua}
REF=${2:-"$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")/.." && pwd)/ref/55/build/lua"}
fail=0
run() { # name, env..., program
  systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout ${TMO:-120} "$@" </dev/null 2>&1 | sed -E 's/0x[0-9a-f]+/PTR/g; s#^[^ :]*lua[^ :]*: ##'
}
for f in tests/lazyci/*.lua; do
  exp=$(run "$REF" "$f")
  T="RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=off RUSTYNAIL_JIT_TRACE=always"
  for mode in "off:RUSTYNAIL_JIT=off" "tr0:$T RUSTYNAIL_JIT_TRACE_LIMITS=lazyci=0" "tr1:$T RUSTYNAIL_JIT_TRACE_LIMITS=lazyci=1" \
              "def1:RUSTYNAIL_JIT_TRACE_LIMITS=lazyci=1" "storm:$T RUSTYNAIL_JIT_TRACE_OPT=+exitstorm=3 RUSTYNAIL_JIT_TRACE_LIMITS=lazyci=1" \
              "storm1:$T RUSTYNAIL_JIT_TRACE_OPT=+exitstorm=1 RUSTYNAIL_JIT_TRACE_LIMITS=lazyci=1" \
              "verify:$T RUSTYNAIL_JIT_TRACE_OPT=+verify RUSTYNAIL_JIT_TRACE_LIMITS=lazyci=1" \
              "gc2:$T RUSTYNAIL_GC_STRESS=2 RUSTYNAIL_JIT_TRACE_LIMITS=lazyci=1" ; do
    name=${mode%%:*}; envs=${mode#*:}
    got=$(env $envs RUSTYNAIL_SEED=0 bash -c 'exec systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout ${TMO:-300} "$0" "$1" </dev/null 2>&1' "$BIN" "$f" | sed -E 's/0x[0-9a-f]+/PTR/g; s#^[^ :]*lua[^ :]*: ##')
    if [ "$exp" != "$got" ]; then
      echo "DIFF $f [$name]"; diff <(echo "$exp") <(echo "$got") | head -8; fail=1
    else
      echo "ok   $f [$name]"
    fi
  done
done
exit $fail
