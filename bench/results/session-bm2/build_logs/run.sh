#!/bin/bash
# Benchmark 2 driver (session-bm2). usage: run.sh <A|B|RERUN> <out-dir> [ONLY=comma list for RERUN]
# Same method as session-2026-11-11 (scripts/bench_session4_run.sh). Subjects: n* = 80f1d96e (new main), p* = 79e7441c (previous subject).
set -u
MODE=$1; OUT=$2
WT=/path/to/worktree
cd $WT
B=$HOME/.cache/bm2
PUC=$HOME/Projects/rustynail/ref/55/build-seed0/lua
e="env RUSTYNAIL_SEED=0"
col() { echo "$1=$e $B/$2/$3/release/lua-cli"; }
SETS=corpus,corpus2/dev,corpus2/heldout,awfy,programs,jit-trace,lib,apps,real
chk() {
  echo "idle check $(date +%T): $(uptime)"
  python3 scripts/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
}
mkdir -p "$OUT"
case $MODE in
A)
  COLS="puc=$PUC;lj=/usr/bin/luajit;$(col nplain 80f1d96e tplain);$(col nplain2 80f1d96e tplain);nint=$e RUSTYNAIL_JIT=off $B/80f1d96e/tplain/release/lua-cli;$(col nnj 80f1d96e tnojit);$(col npgo 80f1d96e pgoout/pgo);$(col pplain 79e7441c tplain);$(col ppgo 79e7441c pgoout/pgo)"
  echo "PASS A START $(date)"; chk
  CPUS=3,4,5,6,7 RUNS=5 OUT=$OUT/passA_cpus3-7_all_columns.json SETS=$SETS COLS="$COLS" python3 scripts/bench_session.py ;;
B)
  COLS="puc=$PUC;lj=/usr/bin/luajit;$(col nplain 80f1d96e tplain);$(col npgo 80f1d96e pgoout/pgo);$(col pplain 79e7441c tplain);$(col ppgo 79e7441c pgoout/pgo)"
  echo "PASS B START $(date)"; chk
  CPUS=4,6 RUNS=5 OUT=$OUT/passB_cpus4-6_fast_lj.json SETS=$SETS COLS="$COLS" python3 scripts/bench_session.py ;;
RERUN)
  COLS="puc=$PUC;$(col nplain 80f1d96e tplain);$(col nplain2 80f1d96e tplain);$(col npgo 80f1d96e pgoout/pgo);$(col pplain 79e7441c tplain);$(col ppgo 79e7441c pgoout/pgo)"
  echo "RERUN START $(date)"; chk
  CPUS=4 RUNS=15 OUT=$OUT/rerun_serial_cpu4_losers_15runs.json SETS=$SETS ONLY="$3" COLS="$COLS" python3 scripts/bench_session.py ;;
esac
echo "END $(date)"; chk
