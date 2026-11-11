#!/bin/bash
# LuaJIT comparison wrapper (memory cap + timeout, stdin /dev/null). See scripts/bench_lj.py.
#   SET=programs OUT=docs/bench-lj-programs.json scripts/bench-lj.sh ref=... ours="env RUSTYNAIL_SEED=0 .../rnlua" lj=/usr/bin/luajit
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
export CPU="${CPU:-4}" RUNS="${RUNS:-7}"
exec systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 \
  timeout "${TIMEOUT:-3000}" python3 "$here/scripts/bench_lj.py" "$@" </dev/null
