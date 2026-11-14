#!/bin/bash
F=<worktree>/bench/results/session-final
P=awfy/towers,lib/co_yield,lib/m_tointeger,lib/sel_n,lib/tonumber_base,real/template_engine,lib/gsub_fn
for pad in z zzzzz zzzzzzzzzzzz zzzzzzzzzzzzzzzzzzzzzzzzzzzzz; do
  BINDIR=<cache>/bin$pad ROUT=sweep_pad${#pad}.json PCPU=4 systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 2400 <cache>/run.sh SWEEP $F "$P" </dev/null > $F/sweep_pad${#pad}.log 2>&1
done
echo done > <cache>/sweep.done
