#!/bin/bash
# same-session baseline: 5a064d7 (previous subject) rebuilt in build_logs/, fast pass vs the cached b116447 columns; retries while the idle check refuses
cd /home/themadmin/Projects/rustynail/.claude/worktrees/agent-a70b4d7021ef0234a
J=bench/results/session-l
for t in $(seq 40); do
  PREV_COMMIT=b116447 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --bindir $HOME/.cache/rnnb/b5a --out $PWD/$J/b5a_fast_all.json > $J/b5a_fast.log 2>&1 </dev/null
  grep -q "busy CPUs" $J/b5a_fast.log || break
  echo "retry $t" >> $J/b5a_retries.log; sleep 20
done
for x in "plain p55" "pgo g55" "p54 p54" "g54 g54"; do set -- $x
  python3 scripts/bench_fast.py import --from $J/b5a_fast_all.json --col $1 --id $2-5a064d7 --commit 5a064d7 --cpus 4 --desc "5a064d7 (session-k subject) rebuilt in session-l, same-session baseline ($1); fast pass only" >> $J/b5a_import.log 2>&1
done
echo FINISHED > $J/b5a_done
