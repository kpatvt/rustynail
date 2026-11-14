#!/bin/bash
F=<worktree>/bench/results/session-bm5
systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 3000 <cache>/rnbm5/run.sh RERUN55 $F "$(cat $F/rerun55_programs.txt)" </dev/null > $F/rerun55.log 2>&1
