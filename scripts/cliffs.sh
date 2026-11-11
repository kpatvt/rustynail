#!/bin/bash
# N5: cycles (Mcyc, min of RUNS cycles:u, pinned) of the bench/lj/cliffs repros, mixed vs uniform.
#   scripts/cliffs.sh <rnlua> [extra env assignments...]
set -u
cd "$(dirname "$0")/.."
L=${1:?rnlua}; shift
RUNS=${RUNS:-3}
one() { # script arg
  local best=
  for r in $(seq "$RUNS"); do
    c=$(env "$@" RUSTYNAIL_SEED=0 taskset -c "${CPU:-4}" perf stat -x, -e cycles:u timeout 120 "$L" "$SCRIPT" "$ARG" 2>&1 >/dev/null </dev/null | grep cycles | cut -d, -f1)
    if [ -z "$best" ] || [ "$c" -lt "$best" ]; then best=$c; fi
  done
  echo $((best / 1000000))
}
for t in "str_unused mixed short long" "str_len mixed short long" "str_eq mixed short long" "call_slot mixed lua c" "num_mix mixed int float" "obj_mix mixed tab"; do
  set -- $t; n=$1; shift
  line="$n"
  for a in "$@"; do SCRIPT=bench/lj/cliffs/$n.lua ARG=$a; line="$line $a=$(one X=1)"; done
  echo "$line"
done
