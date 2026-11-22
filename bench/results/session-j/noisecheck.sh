#!/bin/bash
# re-measure two noisy programs with the subject, PUC and LuaJIT, interleaved, 12 runs each, CPU 4 (cycles in M)
cd /home/themadmin/Projects/rustynail/.claude/worktrees/agent-a98a7cb14f448e248
R=/home/themadmin/Projects/rustynail/ref
export RUSTYNAIL_SEED=0
for p in corpus2/heldout/prototype_objects corpus2/dev/csv_report; do
  for i in $(seq 12); do
    for e in "p55:$HOME/.cache/rnnb/bin/p55/rnlua" "puc55:$R/55/build-seed0/lua" "lj:/usr/bin/luajit"; do
      n=${e%%:*}; b=${e#*:}
      f=bench/$p.lua; [ $n = lj ] && f=bench/lj/$(basename $p).lua
      [ -f "$f" ] || f=bench/$p.lua
      c=$(taskset -c 4 perf stat -x, -e cycles:u $b $f 2>&1 >/dev/null </dev/null | grep cycles | cut -d, -f1)
      echo "$p $n $i $((c/1000000))"
    done
  done
done
