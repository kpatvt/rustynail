#!/bin/bash
# The trace-compiler mutant runner (tr_mutants::every_registered_mutant_is_caught) built once and run in N shards inside ONE
# memory-capped, CPU-limited scope (docs/JIT_TESTING.md "Running the full runner"); the shard result files are aggregated.
#
#   scripts/gate-mutants.sh [--only PATTERN] [--shards N] [--version 5.4]
#     --only   comma list of substrings of mutant names (TR_MUT_ONLY): a group run, e.g. --only cofuse_,amt_
#     --shards number of shard processes (default 1: ONE process whose pool of MUT_JOBS mutants balances dynamically; static shards
#              (TR_MUT_SHARD=i/n, index modulo n) leave some idle while one ends with the slow mutants)
#
# Env: MUT_JOBS (TR_MUT_JOBS per shard, default CPUs/(2*shards): mutants in flight), MUT_CORE_JOBS (TR_CORE_JOBS, default 2: workers of
#      each test binary), MUT_TIMES (file `name<TAB>seconds`, default scripts/gate-mutant-times.tsv: the longest mutants start first;
#      MUT_SAVE_TIMES=1 rewrites it from a full run), GATE_CPUS (default 2-15),
#      GATE_MEM_BIG (the cap of the whole scope, default 12G), MUT_TIMEOUT (seconds, default 14400), TR_MUT_STRICT, TR_MUT_SKIP, ...
#      are passed through. Survivors listed in scripts/gate-known.txt (class `mutant`) are reported as KNOWN.
# Exit status: 0 when every mutant that ran was caught (or only known survivors) and no shard reported a problem.
GATE_NAME=gate-mutants
ONLY=${MUT_ONLY:-}
SHARDS=${MUT_SHARDS:-}
VERSION=5.5
while [ $# -gt 0 ]; do
  case "$1" in
    --only) ONLY=$2; shift 2 ;;
    --shards) SHARDS=$2; shift 2 ;;
    --version) VERSION=$2; shift 2 ;;
    --debug|--fast|--release) export GATE_PROFILE=${1#--}; shift ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
. "$(dirname "$0")/gate-lib.sh"
NC=$(gate_ncpus "$GATE_CPUS")
[ -z "$SHARDS" ] && SHARDS=1
MJOBS=${MUT_JOBS:-$(( NC / (2 * SHARDS) ))}
[ "$MJOBS" -lt 1 ] && MJOBS=1
gate_export_test_env "$VERSION"
FEAT=tr_mutants,t15_oracle
echo "gate-mutants: only=${ONLY:-all} shards=$SHARDS jobs/shard=$MJOBS cpus=${GATE_CPUS:-all} ($NC) mem=$GATE_MEM_BIG version=$VERSION"
gate_step mutants-build 3000 - - cargo test -p rn-jit "${GATE_REL[@]}" --features $FEAT --no-run || { gate_summary; exit 1; }

SHARD_SCRIPT='
shards=$1; d=$2; feat=$3; rel=$4
for i in $(seq 0 $((shards - 1))); do
  ( TR_MUT_SHARD=$i/$shards TR_MUT_RESULTS=$d/shard$i.res \
    cargo test -p rn-jit $rel --features "$feat" --test tr_mutants -- --nocapture every_registered_mutant_is_caught \
    </dev/null >"$d/shard$i.log" 2>&1
    echo $? >"$d/shard$i.rc" ) &
done
wait'

D=$GATE_DIR/mutants
mkdir -p "$D"
S=$(date +%s)
gate_wait_load
[ -n "$ONLY" ] && export TR_MUT_ONLY=$ONLY
export TR_MUT_JOBS=$MJOBS TR_CORE_JOBS=${MUT_CORE_JOBS:-2}
TIMES=${MUT_TIMES:-$GATE_ROOT/scripts/gate-mutant-times.tsv}
[ -s "$TIMES" ] && export TR_MUT_ORDER=$TIMES
gate_cap "$GATE_MEM_BIG" "${GATE_CPUS:--}" timeout "${MUT_TIMEOUT:-14400}" bash -c "$SHARD_SCRIPT" _ "$SHARDS" "$D" "$FEAT" "${GATE_REL[*]}" </dev/null >/dev/null 2>&1
DUR=$(( $(date +%s) - S ))
cat "$D"/shard*.res 2>/dev/null | sort > "$D/all.res"
awk -F'\t' 'NF >= 4 {print $1 "\t" $4}' "$D/all.res" | sort -t"$(printf '\t')" -k2 -rn > "$D/times.tsv"
RAN=$(wc -l < "$D/all.res")
# refresh the times file only from a complete run (no pattern, no shard that died before its mutants were all recorded)
if [ "${MUT_SAVE_TIMES:-0}" = 1 ] && [ -z "$ONLY" ] && [ "$RAN" -ge 600 ]; then cp "$D/times.tsv" "$TIMES"; echo "gate-mutants: refreshed $TIMES ($RAN mutants)"; fi
CAUGHT=$(awk -F'\t' '$2=="caught"' "$D/all.res" | wc -l)
awk -F'\t' '$2!="caught" {print $1}' "$D/all.res" > "$D/survivors.txt"
KRE=$(gate_known_re mutant)
if [ -n "$KRE" ]; then
  grep -E "^($KRE)\$" "$D/survivors.txt" > "$D/known.txt"
  grep -vE "^($KRE)\$" "$D/survivors.txt" > "$D/unknown.txt"
else
  : > "$D/known.txt"
  cp "$D/survivors.txt" "$D/unknown.txt"
fi
NK=$(wc -l < "$D/known.txt")
NU=$(wc -l < "$D/unknown.txt")
BADSHARD=0
for i in $(seq 0 $((SHARDS - 1))); do
  [ "$(cat "$D/shard$i.rc" 2>/dev/null || echo missing)" = 0 ] || BADSHARD=$((BADSHARD + 1))
done
NOTE="$RAN ran, $CAUGHT caught, survivors known=$NK unknown=$NU, failed shards=$BADSHARD of $SHARDS, peak ${GATE_LAST_PEAK_MB:-?}MB; $D"
if [ "$RAN" = 0 ]; then
  gate_record mutants FAIL "$DUR" "no mutant ran (pattern '${ONLY:-}' matches nothing, or the shards died?) $D"
elif [ "$NU" -gt 0 ]; then
  gate_record mutants FAIL "$DUR" "$NOTE (survivors: $(paste -sd, "$D/unknown.txt"))"
elif [ "$BADSHARD" -gt 0 ] && [ "$NK" = 0 ]; then
  gate_record mutants FAIL "$DUR" "$NOTE (shard problem without a survivor: see $D/shard*.log)"
elif [ "$NK" -gt 0 ]; then
  gate_record mutants KNOWN "$DUR" "$NOTE ($(paste -sd, "$D/known.txt"))"
else
  gate_record mutants PASS "$DUR" "$NOTE"
fi
gate_summary
