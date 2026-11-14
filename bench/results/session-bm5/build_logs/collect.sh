#!/bin/bash
W=<worktree>
OUT=$W/bench/results/session-bm5
V=<cache>/rnbm5
mkdir -p $OUT/build_logs
cd $OUT
for f in build_all.sh setup_bin.sh run.sh chain.sh rerun55.sh rerun54.sh startup_all.sh startup.py hf.sh hfoff.sh hfsum.py report.py rerun_list.py rerun54_list.py collect.sh; do cp $V/$f build_logs/; done
cp $V/build_all.log build_logs/
for f in rel-p55 rel-p54 rel-g55 rel-g54 nj55 nj54 rel-b3p rel-b3g; do tail -n 25 $V/logs/$f.log > build_logs/$f-tail.txt; done
for f in rel-g55 rel-g54 rel-b3g; do echo "$f: $(grep -h 'training:' $V/logs/$f.log)"; grep -h "training run failed" $V/logs/$f.log; done > build_logs/pgo_training.txt
{
  cd $V
  for b in dist-p55/rnlua dist-g55/rnlua dist-p54/rnlua dist-g54/rnlua dist-b3p/rnlua dist-b3g/rnlua tnj55/release/rnlua tnj54/release/rnlua; do
    echo "$b: $($b --version </dev/null | head -n 1)"
  done
  for d in dist-p55 dist-g55 dist-p54 dist-g54 dist-b3p dist-b3g; do (cd $d && echo $d && sha256sum -c SHA256SUMS | tr '\n' ' '; echo); done
  for d in p55 g55 p54 g54; do echo "== dist-$d"; ls -l $V/dist-$d | awk '{print $5, $9}'; cat $V/dist-$d/SHA256SUMS; done
  echo "--- rnlua --version (plain 5.5, 5.4), --license head, rnluac --license head"
  $V/dist-p55/rnlua --version </dev/null
  $V/dist-p55/rnlua --license </dev/null | head -n 4
  $V/dist-p54/rnlua --version </dev/null
  $V/dist-p55/rnluac --license </dev/null | head -n 2
  echo "--- sizes: shipped (stripped by the release script) and the unstripped cargo outputs (optimized + debuginfo)"
  for p in dist-p55 dist-g55 dist-p54 dist-g54 dist-b3p dist-b3g; do echo "$p rnlua $(stat -c %s $V/$p/rnlua) rnluac $(stat -c %s $V/$p/rnluac)"; done
  echo "unstripped plain 5.5 $(stat -c %s $V/s55/target/release/rnlua) plain 5.4 $(stat -c %s $V/s55/target-54/release/rnlua) PGO 5.5 $(stat -c %s $V/s55/target/pgo-jit/pgo/release/rnlua) PGO 5.4 $(stat -c %s $V/s55/target-54/pgo-jit/pgo/release/rnlua)"
  echo "nojit builds (unstripped) 5.5 $(stat -c %s $V/tnj55/release/rnlua) 5.4 $(stat -c %s $V/tnj54/release/rnlua); stripped copies used in the benchmark: $(stat -c %s $V/bin/n55/rnlua) $(stat -c %s $V/bin/n54/rnlua)"
  echo "--- reference sizes"; ls -l <repo>/ref/55/build-seed0/lua <repo>/ref/54/build-seed0/lua /usr/bin/luajit | awk '{print $5, $9}'
  echo "--- file"; file $V/dist-p55/rnlua | cut -c1-200
} > $OUT/release_script_check.txt 2>&1
# scrub machine-specific paths in every committed text file (same placeholders as session-release)
cd $OUT
for f in $(grep -rIl "/home/" . ); do
  sed -i -e "s#$W#<worktree>#g" -e 's#<repo>/ref#<repo>/ref#g' -e 's#<repo>#<repo>#g' -e 's#<cache>#<cache>#g' -e 's#\<cache>#<cache>#g' $f
done
