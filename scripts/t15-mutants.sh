#!/usr/bin/env bash
# Mutation matrix of the tier-1.5 region generator (docs/JIT_TIER15.md section 22): for every mutant of
# crates/rn-jit/src/t15/mutants.rs, which verification tool detects it?
#
#   scripts/t15-mutants.sh [--out FILE] [mutant ...]       (default: all mutants, table to target/t15-mutants.md)
#
# Builds the test binaries with the cargo feature `t15_mutants` (separate target dir target-mut), then runs each
# mutant (RUSTYNAIL_JIT_MUT=name) against the tools below. A tool "detects" the mutant when its run fails (assertion,
# abort or signal) or times out. Columns:
#   core     tests/t15_core.rs (twin corpus x allocation modes x budgets, instruction-count twin, emulator tests)
#   policy   tests/t15_policy.rs (anti-thrash policy, respecialisation); only run for pol_* / gate_* mutants ('.' = not run)
#   shadow   tests/t15_stress.rs s_shadow*, s_budget_shadow     (+shadow, alone and combined); S = the shadow check itself
#            aborted (SHADOW VERIFICATION FAILED), D = only the twin/instruction-count comparison of those runs failed
#   storm    tests/t15_stress.rs s_storm*, s_budget_storm       (+exitstorm=1,2,7,61, alone and combined)
#   misc     tests/t15_stress.rs s_randra*, s_flushall, s_verify*  (+randra, +flushall, +verify)
# SHADOW_ONLY=1 prints only the shadow column (S/D/-).
# Every run is capped (8 GB, no swap), niced, with stdin from /dev/null and at most 2 test threads.
set -uo pipefail
cd "$(dirname "$0")/.."
OUT=target/t15-mutants.md
if [ "${1:-}" = --out ]; then OUT=$2; shift 2; fi
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$PWD/target-mut}
CAP=(systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0)
"${CAP[@]}" true </dev/null 2>/dev/null || CAP=()
mkdir -p "$(dirname "$OUT")"
BUILD=$("${CAP[@]}" nice -n 10 timeout 3000 cargo test -p rn-jit --features jit,t15_mutants \
  --test t15_core --test t15_policy --test t15_stress --no-run </dev/null 2>&1) || { echo "$BUILD"; exit 1; }
bin() { echo "$BUILD" | sed -n "s/^ *Executable tests\/$1\.rs (\(.*\))$/\1/p"; }
CORE=$(bin t15_core); POLICY=$(bin t15_policy); STRESS=$(bin t15_stress)
if [ $# -gt 0 ]; then MUTS=("$@"); else
  mapfile -t MUTS < <(sed -n 's/^\/\/! | `\([a-z0-9_]*\)` |.*/\1/p' crates/rn-jit/src/t15/mutants.rs | grep -v '^bad_fall_targets$')
fi
LOG=$(mktemp)
trap 'rm -f "$LOG"' EXIT
run() { # binary args... -> D (detected) or -; $LOG keeps the output
  if env RUSTYNAIL_JIT_MUT="$MUT" "${CAP[@]}" nice -n 10 timeout 100 "$@" --test-threads=2 --quiet </dev/null >"$LOG" 2>&1; then echo -; else echo D; fi
}
{
  echo "| mutant | core | policy | shadow | storm | misc |"
  echo "|---|---|---|---|---|---|"
} >"$OUT"
for MUT in "${MUTS[@]}"; do
  if [ "${SHADOW_ONLY:-0}" = 1 ]; then
    s=$(run "$STRESS" s_shadow s_budget_shadow --nocapture)
    if [ "$s" = D ] && grep -q "SHADOW VERIFICATION FAILED" "$LOG"; then s=S; fi
    echo "| \`$MUT\` | $s |" | tee -a "$OUT"; continue
  fi
  c=$(run "$CORE" --skip every_mutant_is_detected)
  case $MUT in pol_*|gate_*) p=$(run "$POLICY");; *) p=.;; esac  # (the policy suite is slow: only for policy mutants)
  s=$(run "$STRESS" s_shadow s_budget_shadow --nocapture)
  if [ "$s" = D ] && grep -q "SHADOW VERIFICATION FAILED" "$LOG"; then s=S; fi  # S: the shadow check itself fired
  e=$(run "$STRESS" s_storm s_budget_storm)
  m=$(run "$STRESS" s_randra s_flushall s_verify)
  echo "| \`$MUT\` | $c | $p | $s | $e | $m |" | tee -a "$OUT"
done
echo "table: $OUT"
