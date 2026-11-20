#!/bin/bash
# Pre-merge / pre-push gate (docs/GATES.md): the light gate, then the long checks on many workers under one cap each:
#   oracle        tr_oracle::oracle_options sharded (scripts/gate-oracle.sh; ORACLE_TESTS adds oracle_exits, oracle_trace_modes)
#   jitdiff       seeds 1..200, modes off tr tron trdef trdefon trverify trstorm (JITDIFF_ENTERS=1)
#   mutants       the full mutant runner, sharded (scripts/gate-mutants.sh)
#   tests-*-oracle  the rn-jit suite with `--features tr_mutants,t15_oracle` on 5.5 and 5.4 (tr_identity, t15_oracle, the identity
#                 tests of tr_core, the registry tests; without the sharded long tests)
# Steps run one after another (one heavy job at a time); each is internally parallel on the CPU set GATE_CPUS (default 2-15).
#
#   scripts/gate-heavy.sh [--no-54] [--seeds 1-200] [--skip-mutants] [--skip-oracle]
# Env: ORACLE_TESTS, ORACLE_SHARDS, MUT_SHARDS, MUT_ONLY, HEAVY_JD_MODES, GATE_ONLY / GATE_SKIP (step names: light, tests-5.5-oracle, oracle,
#      jitdiff-1-200, mutants; the light gate's own steps are named in its summary), GATE_CPUS, GATE_MEM_BIG.
# Exit status 0 when every step passed (KNOWN failures of scripts/gate-known.txt do not fail it).
GATE_NAME=gate-heavy
NO54=()
SEEDS=1-200
SKIPM=0
SKIPO=0
while [ $# -gt 0 ]; do
  case "$1" in
    --no-54) NO54=(--no-54); shift ;;
    --seeds) SEEDS=$2; shift 2 ;;
    --skip-mutants) SKIPM=1; shift ;;
    --skip-oracle) SKIPO=1; shift ;;
    -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
. "$(dirname "$0")/gate-lib.sh"
. "$GATE_ROOT/scripts/gate-steps.sh"

# the light gate; its jit-diff 1..50 is covered by the 1..200 run below
GATE_SKIP_SAVE=$GATE_SKIP
GATE_SKIP="$GATE_SKIP,jitdiff-1-50" gate_sub light "$GATE_ROOT/scripts/gate-light.sh" "${NO54[@]}"
GATE_SKIP=$GATE_SKIP_SAVE
gate_jit_suite 5.5 oracle
[ "${#NO54[@]}" = 0 ] && gate_jit_suite 5.4 oracle
[ "$SKIPO" = 0 ] && gate_sub oracle "$GATE_ROOT/scripts/gate-oracle.sh"
gate_jitdiff "jitdiff-${SEEDS}" "${SEEDS%-*}" "${SEEDS#*-}" "${HEAVY_JD_MODES:-off tr tron trdef trdefon trverify trstorm}" 12
[ "$SKIPM" = 0 ] && gate_sub mutants "$GATE_ROOT/scripts/gate-mutants.sh"
gate_summary
