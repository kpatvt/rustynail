#!/bin/bash
# subject 7413bff vs the cached 5a064d7 columns; retries while the idle check refuses
cd /home/themadmin/Projects/rustynail/.claude/worktrees/agent-a70b4d7021ef0234a
J=bench/results/session-l
for t in $(seq 40); do
  PREV_COMMIT=5a064d7 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --out $PWD/$J/fast_all.json > $J/fast.log 2>&1 </dev/null
  grep -q "busy CPUs" $J/fast.log || break
  echo "retry $t" >> $J/fast_retries.log; sleep 20
done
echo FINISHED > $J/fast_done
