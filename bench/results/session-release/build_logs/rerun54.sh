#!/bin/bash
F=/path/to/worktree/bench/results/session-release
exec systemd-run --user --scope -q -p MemoryMax=12G -p MemorySwapMax=0 timeout 3000 /path/to/cache/vcBuild/run.sh RERUN54 $F "$(cat $F/rerun54_programs.txt)" </dev/null > $F/rerun54.log 2>&1
