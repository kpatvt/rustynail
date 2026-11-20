#!/bin/bash
# Runs the rn-jit test binaries of a `cargo test --no-run` build several at a time (cargo runs them one after another, so the wall
# time is the SUM of the binaries' longest tests; run concurrently it is about the longest binary). Used by scripts/gate-steps.sh.
#
#   scripts/gate-runbins.sh <list> <parallel> <logdir> <test-threads> [libtest args...]
#
# <list>: lines "<name> <executable path>" (name `lib` = the unit tests). Output is written like cargo's: a `Running` header per binary,
# its output, and `error: test failed` after a failing one, so that scripts/gate-lib.sh (gate_known_test_failures) can read it.
# The binaries run with the working directory of cargo test (crates/rn-jit). GATE_THREADS_BIG (default 8): test threads of tr_core,
# tr_identity, t15_oracle. Exit status: 1 when any binary failed.
list=$1 par=$2 logdir=$3 threads=$4
shift 4
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p "$logdir"
cd "$here/crates/rn-jit" || exit 2
export GR_THREADS=$threads GR_BIG=${GATE_THREADS_BIG:-8} GR_DIR=$logdir GR_ARGS="$*"
# the long binaries first (a short one never ends the run alone)
{ grep -E '^(tr_identity|t15_oracle|tr_core|tr_oracle) ' "$list"; grep -vE '^(tr_identity|t15_oracle|tr_core|tr_oracle) ' "$list"; } |
  xargs -P "$par" -L1 bash -c '
    name=$0; path=$1; t=$GR_THREADS
    # the binaries with many long tests (tr_core: the longest test is 60 s, 220 tests) get more test threads
    case $name in tr_core|tr_identity|t15_oracle) t=$GR_BIG;; esac
    # shellcheck disable=SC2086
    "$path" --test-threads="$t" $GR_ARGS </dev/null >"$GR_DIR/$name.log" 2>&1
    echo $? >"$GR_DIR/$name.rc"'
rc=0
while read -r name path; do
  [ -z "$name" ] && continue
  if [ "$name" = lib ]; then echo "     Running unittests src/lib.rs ($path)"; else echo "     Running tests/$name.rs ($path)"; fi
  cat "$logdir/$name.log"
  if [ "$(cat "$logdir/$name.rc" 2>/dev/null || echo 1)" != 0 ]; then echo "error: test failed, to rerun pass '-p rn-jit --test $name'"; rc=1; fi
done <"$list"
exit $rc
