#!/bin/bash
# VC (session-release) driver. usage: run.sh <A|B|C|RERUN55|RERUN54> <out-dir> [ONLY]
#  A  = 5.5 pass A (CPUs 3-7, all 223 programs): puc lj plain plain2(dup) int nojit pgo bm3plain bm3pgo
#  B  = 5.5 pass B (CPUs 4,6): puc lj plain pgo bm3plain bm3pgo
#  C  = 5.4 pass (CPUs 4,6, all programs): puc(=PUC 5.4) ref55(=PUC 5.5) lj p54 g54 i54(JIT off) n54(nojit) p55 g55
set -u
MODE=$1; OUT=$2
WT=/path/to/worktree
cd $WT
B=$HOME/.cache/vcBuild/bin
R=/path/to/repo/ref
e="env RUSTYNAIL_SEED=0"
col() { echo "$1=$e $B/$2/rnlua"; }
SETS=corpus,corpus2/dev,corpus2/heldout,awfy,programs,jit-trace,lib,apps,real
export SPIN_MS=300
chk() { echo "idle check $(date +%T): $(uptime)"; python3 scripts/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3; }
mkdir -p "$OUT"
case $MODE in
A)
  COLS="puc=$R/55/build-seed0/lua;lj=/usr/bin/luajit;$(col plain p55);$(col plain2 p55);int=$e RUSTYNAIL_JIT=off $B/p55/rnlua;$(col nj n55);$(col pgo g55);$(col b3plain b3p);$(col b3pgo b3g)"
  echo "PASS A START $(date)"; chk
  CPUS=3,4,5,6,7 RUNS=5 OUT=$OUT/passA_cpus3-7_all_columns.json SETS=$SETS COLS="$COLS" python3 scripts/bench_session.py ;;
B)
  COLS="puc=$R/55/build-seed0/lua;lj=/usr/bin/luajit;$(col plain p55);$(col pgo g55);$(col b3plain b3p);$(col b3pgo b3g)"
  echo "PASS B START $(date)"; chk
  CPUS=4,6 RUNS=5 OUT=$OUT/passB_cpus4-6_fast_lj.json SETS=$SETS COLS="$COLS" python3 scripts/bench_session.py ;;
C)
  COLS="puc=$R/54/build-seed0/lua;ref55=$R/55/build-seed0/lua;lj=/usr/bin/luajit;$(col p54 p54);$(col g54 g54);i54=$e RUSTYNAIL_JIT=off $B/p54/rnlua;$(col n54 n54);$(col p55 p55);$(col g55 g55)"
  echo "PASS C (5.4) START $(date)"; chk
  CPUS=4,6 RUNS=5 OUT=$OUT/pass54_cpus4-6.json SETS=$SETS COLS="$COLS" python3 scripts/bench_session.py ;;
RERUN55)
  COLS="puc=$R/55/build-seed0/lua;$(col plain p55);$(col plain2 p55);$(col pgo g55);$(col b3plain b3p);$(col b3pgo b3g)"
  echo "RERUN55 START $(date)"; chk
  CPUS=4 RUNS=15 OUT=$OUT/rerun55_serial_cpu4_15runs.json SETS=$SETS ONLY="$3" COLS="$COLS" python3 scripts/bench_session.py ;;
RERUN54)
  COLS="puc=$R/54/build-seed0/lua;$(col p54 p54);$(col p54b p54);$(col g54 g54)"
  echo "RERUN54 START $(date)"; chk
  CPUS=4 RUNS=15 OUT=$OUT/rerun54_serial_cpu4_15runs.json SETS=$SETS ONLY="$3" COLS="$COLS" python3 scripts/bench_session.py ;;
esac
echo "END $(date)"; chk
