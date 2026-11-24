#!/bin/bash
export PATH="$HOME/.cargo/bin:$PATH"; export CARGO_BUILD_JOBS=8
cd /home/themadmin/Projects/rustynail
V=$HOME/.cache/rnnb; mkdir -p $V/logs $V/bin-m/{p55,g55,p54,g54}
for spec in "p55 --no-pgo" "p54 --no-pgo --lua54" "g55" "g54 --lua54"; do
 set -- $spec; o=$1; shift
 echo "$(date +%T) $o"
 systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 2400 scripts/release-build.sh "$@" --out $V/dist-m-$o > $V/logs/rel-m-$o.log 2>&1 </dev/null || { echo FAIL $o; exit 1; }
 cp $V/dist-m-$o/rnlua $V/bin-m/$o/rnlua
done
echo ALLDONE
