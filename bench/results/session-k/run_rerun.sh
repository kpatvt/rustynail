#!/bin/bash
# 15 serial runs on CPU 4 for the flagged programs: subject first, then b116447 on the same list (like for like). Retries while the idle check refuses.
cd /home/themadmin/Projects/rustynail/.claude/worktrees/agent-ab1a356ca46bcacc8
J=bench/results/session-k
ONLY=$(paste -sd, $J/rerun_programs_all.txt)
run() {
  local name=$1; shift
  for t in $(seq 60); do
    "$@" > $J/$name.log 2>&1 </dev/null
    grep -q "busy CPUs" $J/$name.log || break
    echo "retry $t" >> $J/${name}_retries.log
    sleep 20
  done
}
run rerun env PREV_COMMIT=b116447 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --all-full --runs 15 --only "$ONLY" --out $PWD/$J/rerun_15runs_cpu4.json
run rerun_b116 env PREV_COMMIT=22670a1 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --bindir $HOME/.cache/rnnb/bjj --all-full --runs 15 --only "$ONLY" --out $PWD/$J/rerun_b116_15runs_cpu4.json
echo FINISHED > $J/rerun_done
