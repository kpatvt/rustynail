#!/bin/bash
F=<worktree>/bench/results/session-final
systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 3600 <cache>/run.sh RERUN55 $F "$(cat $F/rerun55_programs.txt)" </dev/null > $F/rerun55.log 2>&1
systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 3600 <cache>/run.sh RERUN54 $F "$(cat $F/rerun54_programs.txt)" </dev/null > $F/rerun54.log 2>&1
echo done > <cache>/rerun.done
