#!/bin/bash
# Post-M full benchmark wrapper: memory cap (whole run, default 12G, no swap) + timeout, stdin /dev/null. See scripts/bench_mfull.py.
# The binaries are fixed paths under ~/.cache/mf-bin (equal length), override with the B_* / PUC / LUAJIT variables.
# Writes its PID to $PIDFILE (default bench-mfull.pid in the worktree) and exits when the queue is empty.
set -eu
here="$(cd "$(dirname "$0")/.." && pwd)"
D="${BINDIR:-$HOME/.cache/mf-bin}"
export PUC="${PUC:-$here/ref/55/build-seed0/lua}" LUAJIT="${LUAJIT:-/usr/bin/luajit}"
export B_PLAIN="${B_PLAIN:-$D/a/rnlua}" B_PGO="${B_PGO:-$D/b/rnlua}" B_PGO2="${B_PGO2:-$D/c/rnlua}"
export B_BPLAIN="${B_BPLAIN:-$D/d/rnlua}" B_BPGO="${B_BPGO:-$D/e/rnlua}"
export CPUS="${CPUS:-4,5,6,7}" RUNS="${RUNS:-11}" MODE="${MODE:-default}"
echo $$ > "${PIDFILE:-$here/bench-mfull.pid}"
exec systemd-run --user --scope -q -p MemoryMax="${MEMMAX:-12G}" -p MemorySwapMax=0 \
  timeout "${TIMEOUT:-14400}" python3 "$here/scripts/bench_mfull.py" </dev/null
