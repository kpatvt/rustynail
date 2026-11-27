#!/bin/bash
cd /home/themadmin/Projects/rustynail
J=bench/results/session-r
export PATH="$HOME/.cargo/bin:$PATH"; export CARGO_BUILD_JOBS=8
V=$HOME/.cache/rnnb; mkdir -p $V/logs $V/bin-r/{p55,g55,p54,g54}
# 2. builds
for spec in "p55 --no-pgo" "p54 --no-pgo --lua54" "g55" "g54 --lua54"; do
 set -- $spec; o=$1; shift
 systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 2400 scripts/release-build.sh "$@" --out $V/dist-r-$o > $V/logs/rel-r-$o.log 2>&1 </dev/null || { echo "BUILD FAIL $o" >> $J/status.txt; exit 1; }
 cp $V/dist-r-$o/rnlua $V/bin-r/$o/rnlua
done
echo builds-ok >> $J/status.txt
# 3. fast pass
for t in $(seq 40); do
  FORCE_STALE=1 PREV_COMMIT=5a064d7 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --bindir $V/bin-r --out $PWD/$J/fast_all.json > $J/fast.log 2>&1 </dev/null
  grep -q "busy CPUs" $J/fast.log || break
  sleep 20
done
echo fast-done >> $J/status.txt
# 4. reruns (9 runs) of the session-n flagged list
sed -e 's#session-n#session-r#g' -e 's#bin-n#bin-r#g' bench/results/session-n/run_rerun.sh > $J/run_rerun.sh
bash $J/run_rerun.sh
echo all-done >> $J/status.txt
