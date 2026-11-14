#!/bin/bash
OUT=<worktree>/bench/results/session-bm5
cd $OUT
echo "startup START $(date)"; python3 <worktree>/scripts/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 1500 python3 <cache>/rnbm5/startup.py $OUT/startup_size.json 50 </dev/null
echo "hyperfine START $(date)"
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 600 <cache>/rnbm5/hf.sh </dev/null
echo "END $(date)"; python3 <worktree>/scripts/cpu_idle.py 3,4,5,6,7,11,12,13,14,15 3
