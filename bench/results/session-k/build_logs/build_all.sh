#!/bin/bash
# session-k builds: subject 5a064d7 (worktree root) -> ~/.cache/rnnb/bin/{p55,g55,p54,g54}; b116447 (scratch-b116 worktree) -> ~/.cache/rnnb/bjj/...
export PATH="$HOME/.cargo/bin:$PATH"; export CARGO_BUILD_JOBS=8
W=/home/themadmin/Projects/rustynail/.claude/worktrees/agent-ab1a356ca46bcacc8
V=$HOME/.cache/rnnb
mkdir -p $V/logs $V/bin/{p55,g55,p54,g54} $V/bjj/{p55,g55,p54,g54}
for sub in "$W s dist-s bin" "$W/scratch-b116 b dist-b bjj"; do
 set -- $sub; cd $1; tag=$2; dd=$3; bd=$4
 for spec in "p55 --no-pgo" "p54 --no-pgo --lua54" "g55" "g54 --lua54"; do
  set -- $spec; o=$1; shift
  echo "$(date +%T) $tag $o"
  systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 1800 scripts/release-build.sh "$@" --out $V/$dd-$o > $V/logs/rel-$tag-$o.log 2>&1 </dev/null || echo FAIL $tag $o
  cp $V/$dd-$o/rnlua $V/$bd/$o/rnlua
 done
done
echo ALLDONE
