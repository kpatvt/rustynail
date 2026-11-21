#!/bin/bash
export PATH="$HOME/.cargo/bin:$PATH"; export CARGO_BUILD_JOBS=8
cd /home/themadmin/Projects/rustynail/.claude/worktrees/agent-a4f86f3c2d1f05d27
V=$HOME/.cache/rnnb
mkdir -p $V/logs
for spec in "p55 --no-pgo" "p54 --no-pgo --lua54" "g55" "g54 --lua54"; do
 set -- $spec; o=$1; shift
 echo "$(date +%T) $o"
 systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 1800 scripts/release-build.sh "$@" --out $V/dist-$o > $V/logs/rel-$o.log 2>&1 </dev/null || echo FAIL $o
done
for d in p55 g55 p54 g54; do cp $V/dist-$d/rnlua $V/bin/$d/rnlua; done
echo ALLDONE
