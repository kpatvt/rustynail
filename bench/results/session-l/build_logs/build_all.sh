#!/bin/bash
# session-l builds: subject 7413bff (worktree root) -> ~/.cache/rnnb/bin/{p55,g55,p54,g54}; 5a064d7 (scratch-5a0 worktree inside the subject worktree (as scratch-b116 in session-k)) -> ~/.cache/rnnb/b5a/...
export PATH="$HOME/.cargo/bin:$PATH"; export CARGO_BUILD_JOBS=8
W=/home/themadmin/Projects/rustynail/.claude/worktrees/agent-a70b4d7021ef0234a
W2=$W/scratch-5a0
V=$HOME/.cache/rnnb
mkdir -p $V/logs $V/bin/{p55,g55,p54,g54} $V/b5a/{p55,g55,p54,g54}
for sub in "$W s dist-s bin" "$W2 b dist-b b5a"; do
 set -- $sub; cd $1; tag=$2; dd=$3; bd=$4
 for spec in "p55 --no-pgo" "p54 --no-pgo --lua54" "g55" "g54 --lua54"; do
  set -- $spec; o=$1; shift
  echo "$(date +%T) $tag $o"
  systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0 timeout 1800 scripts/release-build.sh "$@" --out $V/$dd-$o > $V/logs/rel-$tag-$o.log 2>&1 </dev/null || echo FAIL $tag $o
  cp $V/$dd-$o/rnlua $V/$bd/$o/rnlua
 done
done
echo ALLDONE
