#!/bin/bash
# Per-commit gate (docs/GATES.md): fmt, clippy (5.5, 5.4), the rn-jit trace suite (`--features trace`) on 5.5 and 5.4 (the
# oracle-feature suite, tr_oracle, tr_identity and the mutant runners are in scripts/gate-heavy.sh), vm_corpus, jit-diff seeds
# 1..50, and the mutant groups selected by the pattern argument. Wall time is printed per step.
#
#   scripts/gate-light.sh [MUTANT_PATTERN[,PATTERN...]] [--no-54] [--seeds 1-50] [--base REF] [--explain] [--no-mutants] [--debug]
#
#   MUTANT_PATTERN   substrings of mutant names (TR_MUT_ONLY), e.g. `cofuse_,amt_`. Without it the groups are chosen from the files that
#                    changed since the base (scripts/gate-mutant-plan.py with scripts/gate-mutant-map.tsv; a small default set when
#                    nothing maps): the chosen patterns and the estimated time are printed
#   --base REF       the commit the change is measured from (default: merge-base with origin/main, or HEAD~1 when that is HEAD)
#   --explain        print the plan (changed files -> mutants, patterns, estimate) and the steps, run nothing
#   --no-mutants     leave the mutant group out
#   --no-54          leave out the 5.4 steps (clippy-5.4, tests-5.4)
#   --seeds A-B      jit-diff range (default 1-50), modes JITDIFF_MODES (default `off tr trdef`)
#   --debug|--fast|--release  profile of the rn-jit test builds (default debug; docs/GATES.md "Debug or release": `fast` = opt 3 with the
#                    debug assertions, `release` = without them, which loses checks and fails one test)
# Env (see scripts/gate-lib.sh for the common ones): GATE_ONLY / GATE_SKIP (comma lists of step names), GATE_THREADS (test threads per binary,
#   default 4), LIGHT_JD_MODES.
# Exit status 0 when every step passed. Heavier checks: scripts/gate-heavy.sh.
GATE_NAME=gate-light
PATTERN=
NO54=0
SEEDS=1-50
BASE=
EXPLAIN=0
NOMUT=0
while [ $# -gt 0 ]; do
  case "$1" in
    --no-54) NO54=1; shift ;;
    --seeds) SEEDS=$2; shift 2 ;;
    --base) BASE=$2; shift 2 ;;
    --explain) EXPLAIN=1; shift ;;
    --no-mutants) NOMUT=1; shift ;;
    --debug|--fast|--release) export GATE_PROFILE=${1#--}; shift ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    -*) echo "unknown argument: $1" >&2; exit 2 ;;
    *) PATTERN=$1; shift ;;
  esac
done
# the mutant plan (before the library: --explain creates no log directory)
PLAN_EST=
if [ "$NOMUT" = 0 ]; then
  HERE="$(cd "$(dirname "$0")" && pwd)"
  if [ -n "$PATTERN" ]; then
    PLAN=$(python3 "$HERE/gate-mutant-plan.py" --patterns "$PATTERN" --jobs "${MUT_JOBS:-7}" 2>&1)
  else
    PLAN=$(python3 "$HERE/gate-mutant-plan.py" ${BASE:+--base "$BASE"} --jobs "${MUT_JOBS:-7}" 2>&1)
  fi
  if [ "$EXPLAIN" = 1 ] || [ -z "$PATTERN" ]; then printf '%s\n' "$PLAN" | grep -vE '^(PATTERNS|ESTIMATE)=' >&2; fi
  PATTERN=$(printf '%s\n' "$PLAN" | sed -n 's/^PATTERNS=//p')
  PLAN_EST=$(printf '%s\n' "$PLAN" | sed -n 's/^ESTIMATE=//p')
fi
if [ "$EXPLAIN" = 1 ]; then
  echo "gate-light plan: profile=${GATE_PROFILE:-debug} 5.4=$([ $NO54 = 1 ] && echo off || echo on) jit-diff=$SEEDS mutants=$(printf '%s' "${PATTERN:-none}" | cut -c1-100) (estimated ${PLAN_EST:-0}s for the group)"
  exit 0
fi
. "$(dirname "$0")/gate-lib.sh"
. "$GATE_ROOT/scripts/gate-steps.sh"

gate_step fmt 300 - - cargo fmt --all --check
gate_step mutant-map 120 - - python3 "$GATE_ROOT/scripts/gen-mutant-map.py" --check
gate_step clippy-5.5 1800 - - cargo clippy --workspace --all-targets -- -D warnings
gate_step clippy-5.5-jit 1800 - - cargo clippy -p rn-jit --all-targets --features tr_mutants,t15_oracle -- -D warnings
if [ "$NO54" = 0 ]; then
  gate_step clippy-5.4 1800 - - "${V54ENV[@]}" cargo clippy --workspace --all-targets -- -D warnings
fi
gate_jit_suite 5.5 trace
[ "$NO54" = 0 ] && gate_jit_suite 5.4 trace
gate_vm_corpus
gate_jitdiff "jitdiff-${SEEDS}" "${SEEDS%-*}" "${SEEDS#*-}" "${LIGHT_JD_MODES:-off tr trdef}" 6
if [ -n "$PATTERN" ]; then
  echo "mutants-group: patterns $PATTERN (estimated ${PLAN_EST:-?}s)"
  gate_sub mutants-group "$GATE_ROOT/scripts/gate-mutants.sh" --only "$PATTERN"
fi
gate_summary
