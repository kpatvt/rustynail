#!/bin/bash
# retries while the idle check refuses (desktop noise)
cd /home/themadmin/Projects/rustynail/.claude/worktrees/agent-ab1a356ca46bcacc8
J=bench/results/session-k
for t in $(seq 40); do
  PREV_COMMIT=b116447 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --out $PWD/$J/fast_all.json > $J/fast.log 2>&1 </dev/null
  grep -q "busy CPUs" $J/fast.log || break
  echo "retry $t" >> $J/fast_retries.log; sleep 20
done
echo FINISHED > $J/fast_done
