#!/bin/bash
OUT=/path/to/worktree/bench/results/session-release
mkdir -p $OUT
for m in A B C; do
  case $m in A) LOG=passA.log;; B) LOG=passB.log;; C) LOG=pass54.log;; esac
  systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 7000 /path/to/cache/vcBuild/run.sh $m $OUT "" </dev/null > $OUT/$LOG 2>&1
  echo "LAUNCH-DONE rc=$?" >> $OUT/$LOG
done
echo CHAIN-DONE > $OUT/chain.done
