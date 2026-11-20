#!/bin/bash
# Per-commit gate (docs/GATES.md): fmt, clippy (5.5, 5.4), the rn-jit trace suite (`--features trace`) on 5.5 and 5.4 (the
# oracle-feature suite, tr_oracle, tr_identity and the mutant runners are in scripts/gate-heavy.sh), vm_corpus, jit-diff seeds
# 1..50, and the mutant groups selected by the pattern argument. Wall time is printed per step.
#
#   scripts/gate-light.sh [MUTANT_PATTERN[,PATTERN...]] [--no-54] [--seeds 1-50]
#
#   MUTANT_PATTERN   substrings of mutant names (TR_MUT_ONLY), e.g. `cofuse_,amt_`; without it no mutant runs
#   --no-54          leave out the 5.4 steps (clippy-5.4, tests-5.4)
#   --seeds A-B      jit-diff range (default 1-50), modes JITDIFF_MODES (default `off tr trdef`)
# Env (see scripts/gate-lib.sh for the common ones): GATE_ONLY / GATE_SKIP (comma lists of step names), GATE_THREADS (test threads per binary,
#   default 4), LIGHT_JD_MODES.
# Exit status 0 when every step passed. Heavier checks: scripts/gate-heavy.sh.
GATE_NAME=gate-light
PATTERN=
NO54=0
SEEDS=1-50
while [ $# -gt 0 ]; do
  case "$1" in
    --no-54) NO54=1; shift ;;
    --seeds) SEEDS=$2; shift 2 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    -*) echo "unknown argument: $1" >&2; exit 2 ;;
    *) PATTERN=$1; shift ;;
  esac
done
. "$(dirname "$0")/gate-lib.sh"
. "$GATE_ROOT/scripts/gate-steps.sh"

gate_step fmt 300 - - cargo fmt --all --check
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
  gate_sub mutants-group "$GATE_ROOT/scripts/gate-mutants.sh" --only "$PATTERN"
fi
gate_summary
