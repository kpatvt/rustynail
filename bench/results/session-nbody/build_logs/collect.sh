#!/bin/bash
cd ~/.cache/rnnb
S=/home/themadmin/Projects/rustynail/.claude/worktrees/agent-aad64bca5daa7f0b7/bench/results/session-nbody
{
for d in p55 g55 p54 g54 b1p b1g; do echo dist-$d; (cd dist-$d && sha256sum -c SHA256SUMS 2>&1 | tr '\n' ' '); echo; done
echo "--- sizes (shipped, stripped)"
for d in p55 g55 p54 g54 b1p b1g; do echo dist-$d rnlua $(stat -c %s dist-$d/rnlua) rnluac $(stat -c %s dist-$d/rnluac); done
echo "nojit stripped copies: 5.5 $(stat -c %s bin/n55/rnlua) 5.4 $(stat -c %s bin/n54/rnlua)"
stat -c "%s %n" /home/themadmin/Projects/rustynail/ref/55/build-seed0/lua /home/themadmin/Projects/rustynail/ref/54/build-seed0/lua
for d in p55 g55 p54 g54 n55 n54 b1p b1g; do echo $d $(bin/$d/rnlua --version </dev/null | grep -o "rnlua [0-9.]* (Lua [0-9.]* compatible; JIT: [a-z ]*)"); done
echo "--- count of 'themadmin' strings in shipped rnlua"
for d in p55 g55 p54 g54 b1p b1g; do echo dist-$d $(strings dist-$d/rnlua | grep -c themadmin); done
} > $S/release_script_check.txt 2>&1
for f in rel-p55 rel-p54 rel-g55 rel-g54 rel-b1p rel-b1g nj55 nj54; do tail -n 25 logs/$f.log > $S/build_logs/$f-tail.txt; done
cp logs/build_all.log $S/build_logs/build_all.log
for f in build_all.sh chain.sh run.sh setup_bin.sh rerun_chain.sh sweep_chain.sh startup_all.sh hf.sh jsbench.py rerun_list.py report_nbody.py extra_calc.py collect.sh an/startup.py; do cp $f $S/build_logs/; done
for f in g55 g54 b1g; do echo "$f: $(grep training logs/rel-$f.log)"; done > $S/build_logs/pgo_training.txt
cd $S/build_logs
rm -f report.py vs_baseline.py rr_check.py hf2.sh hfsum.py edit_report.py
cp ../../session-final/build_logs/rerun54_list.py .
ls
