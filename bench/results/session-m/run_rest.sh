#!/bin/bash
cd /home/themadmin/Projects/rustynail
J=bench/results/session-m
while [ ! -f $J/fast_done ]; do sleep 30; done
(cd $J && python3 pick_reruns.py > pick.log 2>&1)
ONLY=$(paste -sd, $J/rerun_programs_all.txt)
run() {
  local name=$1; shift
  for t in $(seq 60); do
    "$@" > $J/$name.log 2>&1 </dev/null
    grep -q "busy CPUs" $J/$name.log || break
    echo "retry $t" >> $J/${name}_retries.log; sleep 20
  done
}
run rerun env FORCE_STALE=1 PREV_COMMIT=5a064d7 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --bindir $HOME/.cache/rnnb/bin-m --all-full --runs 15 --only "$ONLY" --out $PWD/$J/rerun_15runs_cpu4.json
run rerun_l env FORCE_STALE=1 PREV_COMMIT=5a064d7 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --bindir $HOME/.cache/rnnb/bin-l --all-full --runs 15 --only "$ONLY" --out $PWD/$J/rerun_7413_15runs_cpu4.json
echo FINISHED > $J/rerun_done
bash $J/jsrun.sh
