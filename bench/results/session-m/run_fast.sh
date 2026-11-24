#!/bin/bash
cd /home/themadmin/Projects/rustynail
J=bench/results/session-m
for t in $(seq 40); do
  FORCE_STALE=1 PREV_COMMIT=5a064d7 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --bindir $HOME/.cache/rnnb/bin-m --out $PWD/$J/fast_all.json > $J/fast.log 2>&1 </dev/null
  grep -q "busy CPUs" $J/fast.log || break
  echo "retry $t" >> $J/fast_retries.log; sleep 20
done
echo FINISHED > $J/fast_done
