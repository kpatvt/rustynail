#!/bin/bash
# retries while the idle check refuses (desktop noise)
cd /home/themadmin/Projects/rustynail/.claude/worktrees/agent-ab1a356ca46bcacc8
J=bench/results/session-k
for t in $(seq 40); do
  PREV_COMMIT=22670a1 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --bindir $HOME/.cache/rnnb/bjj --out $PWD/$J/b116_fast_all.json > $J/b116_fast.log 2>&1 </dev/null
  grep -q "busy CPUs" $J/b116_fast.log || break
  echo "retry $t" >> $J/b116_retries.log; sleep 20
done
echo FINISHED > $J/b116_done
