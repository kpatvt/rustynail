#!/bin/bash
cd /home/themadmin/Projects/rustynail
J=bench/results/session-n
export PATH="$HOME/.cargo/bin:$PATH"; export CARGO_BUILD_JOBS=8
V=$HOME/.cache/rnnb; mkdir -p $V/logs $V/bin-n/{p55,g55,p54,g54}
# 1. batch verification
GATE_CPUS=2-15 timeout 3000 scripts/gate-light.sh --fast </dev/null > $J/gate_light.log 2>&1; echo "gate-light exit $?" >> $J/status.txt
# 2. builds
for spec in "p55 --no-pgo" "p54 --no-pgo --lua54" "g55" "g54 --lua54"; do
 set -- $spec; o=$1; shift
 systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 2400 scripts/release-build.sh "$@" --out $V/dist-n-$o > $V/logs/rel-n-$o.log 2>&1 </dev/null || { echo "BUILD FAIL $o" >> $J/status.txt; exit 1; }
 cp $V/dist-n-$o/rnlua $V/bin-n/$o/rnlua
done
echo builds-ok >> $J/status.txt
# 3. fast pass
for t in $(seq 40); do
  FORCE_STALE=1 PREV_COMMIT=5a064d7 scripts/bench-fast.sh --matrix 55plain,55pgo,54plain,54pgo --bindir $V/bin-n --out $PWD/$J/fast_all.json > $J/fast.log 2>&1 </dev/null
  grep -q "busy CPUs" $J/fast.log || break
  sleep 20
done
echo fast-done >> $J/status.txt
