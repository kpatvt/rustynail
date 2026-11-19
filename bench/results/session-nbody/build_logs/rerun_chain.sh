#!/bin/bash
F=/home/themadmin/Projects/rustynail/.claude/worktrees/agent-aad64bca5daa7f0b7/bench/results/session-nbody
V=$HOME/.cache/rnnb
{ echo "pre-rerun state $(date +%T):"; uptime; free -h | head -3; ps -eo pcpu,args --sort=-pcpu | head -6 | cut -c1-120; } > $F/prestate_rerun.txt
systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 5400 $V/run.sh RERUN55 $F "$(cat $F/rerun55_programs.txt)" </dev/null > $F/rerun55.log 2>&1
systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 3600 $V/run.sh RERUN54 $F "$(cat $F/rerun54_programs.txt)" </dev/null > $F/rerun54.log 2>&1
echo done > $V/rerun.done
