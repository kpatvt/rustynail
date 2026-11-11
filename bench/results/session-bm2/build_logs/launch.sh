#!/bin/bash
# usage: launch.sh <A|B|RERUN> [ONLY]
OUT=/path/to/worktree/bench/results/session-bm2
mkdir -p $OUT
case $1 in A) LOG=passA.log;; B) LOG=passB.log;; RERUN) LOG=rerun.log;; esac
systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 6000 /path/to/cache/bm2/run.sh $1 $OUT "${2:-}" </dev/null > $OUT/$LOG 2>&1
echo "LAUNCH-DONE rc=$?" >> $OUT/$LOG
