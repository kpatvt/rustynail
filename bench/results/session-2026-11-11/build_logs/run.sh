#!/bin/bash
# Session 2026-11-11 benchmark driver. usage: bench_session4_run.sh <A|B|RERUN> <out-dir> [ONLY=comma list for RERUN]
# Binaries live in equal-length directories ~/.cache/bench1111/<hash8>/{tplain,tnojit,pgoout/pgo}/release/lua-cli (built by
# ~/.cache/bench1111/build.sh), all columns run the same script paths (this worktree's bench/). Every ours column gets RUSTYNAIL_SEED=0
# (bench_session.py pads the environment to equal length).
set -u
MODE=$1; OUT=$2
cd /path/to/repo
B=$HOME/.cache/bench1111
PUC=$HOME/Projects/rustynail/ref/55/build-seed0/lua
e="env RUSTYNAIL_SEED=0"
col() { echo "$1=$e $B/$2/$3/release/lua-cli"; }
SETS=corpus,corpus2/dev,corpus2/heldout,awfy,programs,jit-trace,lib,apps,real
chk() { # idle check of benchmark CPUs and siblings: print per-cpu busy% over 3 s
  echo "idle check $(date +%T): $(uptime)"
  python3 scripts/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
}
mkdir -p "$OUT"
case $MODE in
A)
  COLS="puc=$PUC;lj=/usr/bin/luajit;$(col nplain 79e7441c tplain);$(col nplain2 79e7441c tplain);nint=$e RUSTYNAIL_JIT=off $B/79e7441c/tplain/release/lua-cli;$(col nnj 79e7441c tnojit);$(col npgo 79e7441c pgoout/pgo);$(col pplain 4ce16b7a tplain);$(col ppgo 4ce16b7a pgoout/pgo);$(col qplain 53133e8d tplain);$(col qpgo 53133e8d pgoout/pgo);$(col mplain 8965e73e tplain);$(col mpgo 8965e73e pgoout/pgo);$(col oplain c276c376 tplain);$(col opgo c276c376 pgoout/pgo);$(col rplain cb1af288 tplain);$(col rpgo cb1af288 pgoout/pgo)"
  echo "PASS A START $(date)"; chk
  CPUS=3,4,5,6,7 RUNS=5 OUT=$OUT/passA_cpus3-7_all_columns.json SETS=$SETS COLS="$COLS" python3 scripts/bench_session.py ;;
B)
  COLS="puc=$PUC;lj=/usr/bin/luajit;$(col nplain 79e7441c tplain);$(col npgo 79e7441c pgoout/pgo);$(col pplain 4ce16b7a tplain);$(col ppgo 4ce16b7a pgoout/pgo);$(col qplain 53133e8d tplain);$(col qpgo 53133e8d pgoout/pgo);$(col mplain 8965e73e tplain);$(col mpgo 8965e73e pgoout/pgo);$(col oplain c276c376 tplain);$(col opgo c276c376 pgoout/pgo);$(col rplain cb1af288 tplain);$(col rpgo cb1af288 pgoout/pgo)"
  echo "PASS B START $(date)"; chk
  CPUS=4,6 RUNS=5 OUT=$OUT/passB_cpus4-6_fast_lj.json SETS=$SETS COLS="$COLS" python3 scripts/bench_session.py ;;
RERUN)
  COLS="puc=$PUC;$(col nplain 79e7441c tplain);$(col nplain2 79e7441c tplain);$(col npgo 79e7441c pgoout/pgo);$(col pplain 4ce16b7a tplain);$(col ppgo 4ce16b7a pgoout/pgo);$(col qplain 53133e8d tplain);$(col qpgo 53133e8d pgoout/pgo)"
  echo "RERUN START $(date)"; chk
  CPUS=4 RUNS=15 OUT=$OUT/rerun_serial_cpu4_losers_15runs.json SETS=$SETS ONLY="$3" COLS="$COLS" python3 scripts/bench_session.py ;;
esac
echo "END $(date)"; chk
