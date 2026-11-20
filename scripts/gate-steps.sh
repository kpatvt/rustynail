#!/bin/bash
# Step library shared by scripts/gate-light.sh and scripts/gate-heavy.sh (source after scripts/gate-lib.sh).

# Tests that are long single jobs: the light gate leaves them out, the heavy gate runs them through scripts/gate-oracle.sh and
# scripts/gate-mutants.sh (sharded, so the total set is the same).
GATE_HEAVY_SKIPS=(--skip mutants_are_caught_by_live_programs --skip every_registered_mutant_is_caught
                  --skip oracle_options --skip oracle_exits --skip oracle_trace_modes)

# gate_jit_suite <5.5|5.4> <trace|oracle>: build + run the rn-jit suites of one Lua version.
#   trace   `--features trace`: the shipped code (the mutant hooks compile to constant false), every test of the crate that is not
#           gated on the test-only features (this is "the rn-jit trace suite" of the light gate)
#   oracle  `--features tr_mutants,t15_oracle`: the oracle-gated tests, tr_identity, the identity tests of tr_core, the mutant registry
#           tests; without the single long tests that have their own sharded steps (GATE_HEAVY_SKIPS)
# Both pass --no-fail-fast; failing tests listed in scripts/gate-known.txt (class test) make the step KNOWN instead of FAIL.
gate_jit_suite() {
  local v=$1 which=$2 pre=() feat skips=() n
  n=tests-$v-$which
  if [ "$v" = 5.4 ]; then pre=("${V54ENV[@]}"); else pre=(env); fi
  if [ "$which" = oracle ]; then feat=tr_mutants,t15_oracle; skips=("${GATE_HEAVY_SKIPS[@]}"); else feat=trace; fi
  gate_step "$n-build" 3000 - - "${pre[@]}" cargo test -p rn-jit --features "$feat" --lib --tests --no-run || return
  [ -f "$GATE_DIR/$n-build.log" ] || return
  # the binaries of the build, run several at a time (scripts/gate-runbins.sh), not one after another as `cargo test` does
  sed -n -e 's/^ *Executable unittests src\/lib.rs (\(.*\))$/lib \1/p' -e 's/^ *Executable tests\/\([a-z0-9_]*\)\.rs (\(.*\))$/\1 \2/p' "$GATE_DIR/$n-build.log" |
    awk -v r="$GATE_ROOT" '{ if (substr($2, 1, 1) != "/") $2 = r "/" $2; print }' > "$GATE_DIR/$n.list"
  # Concurrency is tuned so that the whole suite keeps the load average at about the CPU count of the set (the first version,
  # 4 binaries x 12 test threads x 6 pool workers, reached load 50 on 14 CPUs and made the 300 s leashes of the oracle jobs unreliable):
  # trace suite: tr_core 10 test threads (load ~15 alone); oracle suite: 3 binaries at a time, 4 threads for the big ones, pools of 3.
  local big=10 p8a=4 par=3
  if [ "$which" = oracle ]; then big=4; p8a=3; fi
  GATE_VERSION=$v GATE_THREADS_BIG=${GATE_THREADS_BIG:-$big} GATE_TESTS=1 TR_P8A_JOBS=${GATE_P8A_JOBS:-$p8a} gate_step "$n" 3000 - "${GATE_CPUS:--}" \
    "$GATE_ROOT/scripts/gate-runbins.sh" "$GATE_DIR/$n.list" "${GATE_BIN_PAR:-$par}" "$GATE_DIR/$n.bins" "${GATE_THREADS:-4}" "${skips[@]}"
}

# vm_corpus + the rn-cli tests against the reference build (release). `ref/` is not tracked: a worktree gets a symlink to the
# main checkout's.
gate_vm_corpus() {
  if [ ! -e "$GATE_ROOT/ref" ]; then
    local common
    common=$(git -C "$GATE_ROOT" rev-parse --git-common-dir 2>/dev/null)
    if [ -n "$common" ] && [ -d "$common/../ref" ]; then ln -s "$(cd "$common/.." && pwd)/ref" "$GATE_ROOT/ref"; fi
  fi
  gate_step vm_corpus 3000 - "${GATE_CPUS:--}" cargo test --release -p rustynail -p rn-cli
}

GATE_REL_BUILT=0
gate_build_release() {
  [ "$GATE_REL_BUILT" = 1 ] && return
  gate_selected build-release || return
  gate_step build-release 1800 - - cargo build --release -p rn-cli && GATE_REL_BUILT=1
}

# gate_jitdiff <step-name> <seed-from> <seed-to> "<modes>" [<jobs>]: scripts/jit-diff.sh on the release rnlua with
# JITDIFF_ENTERS=1 (the counter assertions: a mode that never ran compiled code / traces fails). Jobs are capped by GATE_CPUS.
gate_jitdiff() {
  local name=$1 from=$2 to=$3 modes=$4 jobs=${5:-6} nc
  nc=$(gate_ncpus "$GATE_CPUS")
  [ "$jobs" -gt "$nc" ] && jobs=$nc
  gate_selected "$name" || return 0
  gate_build_release
  GATE_JD_FAILS=0
  JITDIFF_CAPPED=1 JITDIFF_MODES="$modes" JITDIFF_ENTERS=1 JOBS=$jobs JITDIFF_ALLOW_MANY=1 \
    gate_step "$name" 14400 - "${GATE_CPUS:--}" "$GATE_ROOT/scripts/jit-diff.sh" "$from" "$to" "$GATE_ROOT/target/release/rnlua"
  # jit-diff.sh prints diffs and exits 0/1; a nonzero exit with "diffs" in the log is a FAIL already recorded
}

# gate_sub <step-name> <script> [args...]: run a sub-gate script (which does its own capping and records its own steps);
# the step is PASS/KNOWN/FAIL like the sub-gate's last row.
gate_sub() {
  local name=$1; shift
  gate_selected "$name" || return 0
  local s=$(date +%s) rc d=$GATE_DIR/$name
  mkdir -p "$d"
  GATE_DIR=$d "$@" </dev/null >"$d/out.log" 2>&1
  rc=$?
  local dur=$(( $(date +%s) - s ))
  local res=PASS
  [ $rc != 0 ] && res=FAIL
  [ $rc = 0 ] && grep -qE '^[a-z_-]+ +KNOWN ' "$d/summary.txt" 2>/dev/null && res=KNOWN
  gate_record "$name" "$res" "$dur" "$d/out.log ($(grep -E ' (PASS|FAIL|KNOWN) ' "$d/summary.txt" 2>/dev/null | awk '{printf "%s=%s ", $1, $2}'))"
}
