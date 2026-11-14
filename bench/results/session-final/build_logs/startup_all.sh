#!/bin/bash
OUT=<worktree>/bench/results/session-final
S=<worktree>/scripts
cd $OUT
echo "startup START $(date)"; python3 $S/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 1500 python3 <cache>/an/startup.py $OUT/startup_size.json 50 </dev/null
echo "hyperfine START $(date)"; python3 $S/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 600 <cache>/hf.sh </dev/null
echo "END $(date)"; python3 $S/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
