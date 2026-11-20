#!/bin/bash
# The tr_oracle tests (oracle_options by default, about 35 minutes with the test's own 3 workers) on many workers, and optionally
# sharded into several processes, under one memory cap and CPU set; the failures of all shards are aggregated and the known
# path/ASLR-dependent ones (scripts/gate-known.txt: core/bb_tset, tests/vm/table_insert_layout) are reported separately as KNOWN.
# A drop-in for `cargo test -p rn-jit --features tr_mutants,t15_oracle --test tr_oracle -- oracle_options`: the whole job set runs.
#
#   scripts/gate-oracle.sh [--tests oracle_options,oracle_exits,oracle_trace_modes] [--shards N] [--workers W] [--version 5.4]
#
# Env: ORACLE_TESTS (default oracle_options), ORACLE_SHARDS (processes, default 1), ORACLE_WORKERS (child workers per process,
#      default: CPUs of the set / shards), GATE_CPUS (default 2-15), GATE_MEM_BIG (default 12G, the cap of the whole scope),
#      TR_P8A_ONLY / TR_P8A_GEN / TR_P8A_TIMEOUT / RUSTYNAIL_JIT_MUT are passed through (a mutant run: the same cap and shards).
# Exit status: 0 when every job passed or only KNOWN jobs failed; 1 otherwise. `inconclusive` jobs (leash timeouts of the tests
# documented in docs/JIT_TESTING.md) are not failures, as in the test itself.
GATE_NAME=gate-oracle
TESTS=${ORACLE_TESTS:-oracle_options}
SHARDS=${ORACLE_SHARDS:-1}
WORKERS=${ORACLE_WORKERS:-}
VERSION=5.5
while [ $# -gt 0 ]; do
  case "$1" in
    --tests) TESTS=$2; shift 2 ;;
    --shards) SHARDS=$2; shift 2 ;;
    --workers) WORKERS=$2; shift 2 ;;
    --version) VERSION=$2; shift 2 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
. "$(dirname "$0")/gate-lib.sh"
NC=$(gate_ncpus "$GATE_CPUS")
[ -z "$WORKERS" ] && WORKERS=$(( NC / SHARDS ))
[ "$WORKERS" -lt 1 ] && WORKERS=1
if [ "$VERSION" = 5.4 ]; then
  export RUSTFLAGS="--cfg rustynail_lua54" CARGO_TARGET_DIR="$GATE_ROOT/target-54"
fi
FEAT=tr_mutants,t15_oracle
echo "gate-oracle: tests=$TESTS shards=$SHARDS workers/shard=$WORKERS cpus=${GATE_CPUS:-all} ($NC) mem=$GATE_MEM_BIG version=$VERSION"
gate_step oracle-build 3000 - - cargo test -p rn-jit --features $FEAT --test tr_oracle --no-run || { gate_summary; exit 1; }
EXE=$(gate_test_bin $FEAT tr_oracle)
[ -x "$EXE" ] || { echo "cannot find the tr_oracle test binary"; exit 2; }

# one shard process (run inside the scope): exe test shards workers dir index
SHARD_SCRIPT='
exe=$1; t=$2; shards=$3; workers=$4; d=$5
for i in $(seq 0 $((shards - 1))); do
  ( if [ "$shards" -gt 1 ]; then export TR_P8A_SHARD=$i/$shards; fi
    TR_P8A_JOBS=$workers "$exe" --exact "$t" --nocapture --test-threads=1 </dev/null >"$d/shard$i.log" 2>&1
    echo $? >"$d/shard$i.rc" ) &
done
wait'

IFS=, read -ra TLIST <<<"$TESTS"
for T in "${TLIST[@]}"; do
  gate_selected "$T" || continue
  S=$(date +%s)
  D=$GATE_DIR/$T
  mkdir -p "$D"
  gate_wait_load
  # one scope: the cap and the CPU set cover all shards together
  gate_cap "$GATE_MEM_BIG" "${GATE_CPUS:--}" timeout "${ORACLE_TIMEOUT:-9000}" bash -c "$SHARD_SCRIPT" _ "$EXE" "$T" "$SHARDS" "$WORKERS" "$D" </dev/null >/dev/null 2>&1
  DUR=$(( $(date +%s) - S ))
  FAILSHARDS=0
  JOBS=0
  INCONC=0
  UNCL=0
  for i in $(seq 0 $((SHARDS - 1))); do
    rc=$(cat "$D/shard$i.rc" 2>/dev/null || echo missing)
    if [ "$rc" != 0 ]; then
      FAILSHARDS=$((FAILSHARDS + 1))
      # a failing shard must have failed by the oracle assertion (labels), not by a crash / other assertion of the harness
      grep -q 'oracle failures:' "$D/shard$i.log" || UNCL=$((UNCL + 1))
    fi
    n=$(sed -n 's/.*\[tr_oracle\] [^:]*: \([0-9]*\) jobs: .* \([0-9]*\) inconclusive;.*/\1 \2/p' "$D/shard$i.log" | head -n 1)
    [ -z "$n" ] && n="0 0"
    JOBS=$((JOBS + ${n%% *}))
    INCONC=$((INCONC + ${n##* }))
  done
  cat "$D"/shard*.log | grep -E '^--- ' | sort -u > "$D/failed-labels.txt"
  KRE=$(gate_known_re oracle)
  if [ -n "$KRE" ]; then
    grep -E "$KRE" "$D/failed-labels.txt" > "$D/known.txt"
    grep -vE "$KRE" "$D/failed-labels.txt" > "$D/unknown.txt"
  else
    : > "$D/known.txt"
    cp "$D/failed-labels.txt" "$D/unknown.txt"
  fi
  NK=$(wc -l < "$D/known.txt")
  NU=$(wc -l < "$D/unknown.txt")
  NOTE="$JOBS jobs, $INCONC inconclusive, $SHARDS shard(s) x $WORKERS workers, peak ${GATE_LAST_PEAK_MB:-?}MB; known=$NK unknown=$NU; $D"
  if [ "$FAILSHARDS" = 0 ]; then
    gate_record "$T" PASS "$DUR" "$NOTE"
  elif [ "$NU" = 0 ] && [ "$UNCL" = 0 ] && [ "$NK" -gt 0 ]; then
    gate_record "$T" KNOWN "$DUR" "$NOTE (failing: $(sed 's/^--- //; s/ \[.*//' "$D/known.txt" | sort -u | paste -sd, -))"
  else
    gate_record "$T" FAIL "$DUR" "$NOTE (unclassified failure: see $D/shard*.log)"
  fi
done
gate_summary
