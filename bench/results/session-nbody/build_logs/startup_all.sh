#!/bin/bash
OUT=/home/themadmin/Projects/rustynail/.claude/worktrees/agent-aad64bca5daa7f0b7/bench/results/session-nbody
S=/home/themadmin/Projects/rustynail/.claude/worktrees/agent-aad64bca5daa7f0b7/scripts
V=$HOME/.cache/rnnb
cd $OUT
echo "startup START $(date)"; python3 $S/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 1500 python3 $V/an/startup.py $OUT/startup_size.json 50 </dev/null
echo "hyperfine START $(date)"; python3 $S/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 900 $V/hf.sh </dev/null
echo "END $(date)"; python3 $S/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
echo done > $V/startup.done
