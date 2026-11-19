#!/bin/bash
F=/home/themadmin/Projects/rustynail/.claude/worktrees/agent-aad64bca5daa7f0b7/bench/results/session-nbody
V=$HOME/.cache/rnnb
P=corpus/parser_combinator,lib/find_cap,lib/fmt_g,lib/fmt_q,lib/tonumber_bad,lib/tunpack
for pad in z zzzzz zzzzzzzzzzzz zzzzzzzzzzzzzzzzzzzzzzzzzzzzz; do
  mkdir -p $V/bin$pad; for d in p55 g55 b1p b1g; do mkdir -p $V/bin$pad/$d; cp $V/bin/$d/rnlua $V/bin$pad/$d/rnlua; done
  BINDIR=$V/bin$pad ROUT=sweep_pad${#pad}.json PCPU=4 systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 2400 $V/run.sh SWEEP $F "$P" </dev/null > $F/sweep_pad${#pad}.log 2>&1
done
echo done > $V/sweep.done
