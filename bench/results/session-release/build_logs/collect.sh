#!/bin/bash
cd /path/to/worktree/bench/results/session-release
mkdir -p build_logs
V=/path/to/cache/vcBuild
cp $V/build_all.sh $V/run.sh $V/chain.sh $V/rerun54.sh $V/report.py $V/startup.py $V/hf.sh $V/chk.sh $V/collect.sh build_logs/
cp $V/build_all.log build_logs/
for f in rel-p55 rel-p54 rel-g55 rel-g54 nj55 nj54 b3p b3g; do tail -25 $V/logs/$f.log > build_logs/$f-tail.txt; done
for f in rel-g55 rel-g54 b3g; do echo "$f: $(grep -h 'training:' $V/logs/$f.log)"; grep -h "training run failed" $V/logs/$f.log; done > build_logs/pgo_training.txt
{
  bash $V/chk.sh
  for d in p55 g55 p54 g54; do echo "== dist-$d"; ls -l $V/dist-$d | awk '{print $5, $9}'; cat $V/dist-$d/SHA256SUMS; done
  echo "--- rnlua --version (plain 5.5, 5.4), --license head, rnluac --license head"
  $V/dist-p55/rnlua --version </dev/null
  $V/dist-p55/rnlua --license </dev/null | head -4
  $V/dist-p54/rnlua --version </dev/null
  $V/dist-p55/rnluac --license </dev/null | head -2
  echo "--- stripped sizes (strip -s of a copy)"
  for d in p55 g55 p54 g54; do cp $V/dist-$d/rnlua /tmp/scratch/strip_$d; strip -s /tmp/scratch/strip_$d; echo "$d unstripped $(stat -c %s $V/dist-$d/rnlua) stripped $(stat -c %s /tmp/scratch/strip_$d)"; rm /tmp/scratch/strip_$d; done
  echo "--- reference sizes"; ls -l /path/to/repo/ref/55/build-seed0/lua /path/to/repo/ref/54/build-seed0/lua /usr/bin/luajit | awk '{print $5, $9}'
  echo "--- nojit builds"; ls -l $V/tnj55/release/rnlua $V/tnj54/release/rnlua | awk '{print $5, $9}'
  echo "--- file"; file $V/dist-p55/rnlua | cut -c1-200
} > release_script_check.txt 2>&1
