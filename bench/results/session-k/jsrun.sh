#!/bin/bash
cd /home/themadmin/Projects/rustynail/.claude/worktrees/agent-ab1a356ca46bcacc8
J=bench/results/session-k
mkdir -p $J/js $J/js20
ps -eo pcpu,comm --sort=-pcpu | head -4 > $J/prestate_js.txt
node --version > $J/js/env.txt; target/release/rnlua -v >> $J/js/env.txt 2>&1; luajit -v >> $J/js/env.txt 2>&1
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 3000 bench/js/run.sh -o $J/js > $J/js.log 2>&1 </dev/null
systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 3000 bench/js/run.sh -s 20 -o $J/js20 > $J/js20.log 2>&1 </dev/null
echo JSDONE > $J/js_done
