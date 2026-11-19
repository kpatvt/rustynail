#!/bin/bash
OUT=/home/themadmin/Projects/rustynail/.claude/worktrees/agent-aad64bca5daa7f0b7/bench/results/session-nbody
V=$HOME/.cache/rnnb
mkdir -p $OUT
for m in A B C; do
  case $m in A) LOG=passA.log;; B) LOG=passB.log;; C) LOG=pass54.log;; esac
  { echo "pre-pass state $(date +%T):"; uptime; free -h | head -3; ps -eo pcpu,args --sort=-pcpu | head -6 | cut -c1-120; } > $OUT/prestate_$m.txt
  systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 9000 $V/run.sh $m $OUT </dev/null > $OUT/$LOG 2>&1
  echo "LAUNCH-DONE rc=$?" >> $OUT/$LOG
done
echo CHAIN-DONE > $V/chain.done
