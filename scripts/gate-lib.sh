#!/bin/bash
# Shared helpers of scripts/gate-*.sh (docs/GATES.md). Source it, do not run it.
#
# Every step runs in its own systemd scope: memory cap (CLAUDE.md), no swap, optional CPU set, timeout, stdin from /dev/null.
# A step is recorded as PASS / FAIL / KNOWN (only the failures listed in scripts/gate-known.txt) / SKIP with its wall time and
# (when the scope is observable) the peak memory of the scope; `gate_summary` prints the table.
#
# Knobs (environment):
#   GATE_CPUS     CPU set of the heavy parallel steps (default: 2-15 when the machine has >= 16 CPUs, else all). The desktop keeps 0-1.
#   GATE_MEM      MemoryMax of an ordinary step (default 8G); GATE_MEM_BIG: MemoryMax of the wide parallel steps (default 12G)
#   GATE_THREADS  `--test-threads` of each test binary (default 4); GATE_BIN_PAR: binaries run at the same time (default 3),
#                 GATE_THREADS_BIG: test threads of tr_core / tr_identity / t15_oracle (10; 4 in the oracle-feature suite)
#   GATE_DIR      where logs and the summary go (default target/gate/<gate name>-<timestamp>)
#   GATE_FASTLINK 1: link the test builds with mold (scripts/fast-build-env.sh; default 0: rustc >= 1.90 already links with rust-lld,
#                 measured: mold gives nothing, docs/GATES.md); GATE_SCCACHE=0 turns the sccache wrapper off
#   GATE_LOAD_MAX load average above which a step waits before it starts (default 24: the "choked" threshold of load 23 is never
#                 reached by one gate; the wait only protects against a second heavy job started elsewhere), 0 disables
GATE_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$GATE_ROOT" || exit 2
export PATH="$HOME/.cargo/bin:$PATH"
NCPU=$(nproc)
if [ -z "${GATE_CPUS:-}" ]; then
  if [ "$NCPU" -ge 16 ]; then GATE_CPUS="2-$((NCPU - 1))"; else GATE_CPUS=""; fi
fi
GATE_MEM=${GATE_MEM:-8G}
GATE_MEM_BIG=${GATE_MEM_BIG:-12G}
GATE_THREADS=${GATE_THREADS:-4}
GATE_LOAD_MAX=${GATE_LOAD_MAX:-24}
GATE_NAME=${GATE_NAME:-gate}
GATE_DIR=${GATE_DIR:-$GATE_ROOT/target/gate/$GATE_NAME-$(date +%Y%m%d-%H%M%S)}
mkdir -p "$GATE_DIR"
GATE_T0=$(date +%s)
GATE_RESULTS=()
GATE_FAILED=0
GATE_ONLY=${GATE_ONLY:-}   # comma list of step names: run only these
GATE_SKIP=${GATE_SKIP:-}   # comma list of step names: leave these out
V54ENV=(env RUSTFLAGS="--cfg rustynail_lua54" CARGO_TARGET_DIR="$GATE_ROOT/target-54")

# Profile of the rn-jit TEST builds (the light/heavy suites, gate-oracle, gate-mutants), GATE_PROFILE / `--debug|--fast|--release` of the gate scripts:
#   fast     the release optimisation level (opt 3) WITH debug assertions and overflow checks, no fat LTO, 16 codegen units:
#            the same checks and cfg(debug_assertions) code paths as `debug` (the trace compiler forces its verification on, the landing
#            cache and the poisoned-descriptor checks run, t15 emits the budget invariant; t15_core_emu::intrinsic_calls_match_tier1_at_every_pc
#            FAILS without them), at the speed of optimised code
#   release  opt 3 WITHOUT debug assertions (the shipped cfg): not equivalent to `debug`, see docs/GATES.md; for experiments
#   debug    (default) the dev profile (opt 1, debug assertions)
# `fast` and `release` use their own target directories (target-gr / target-54r) so that they never invalidate the shipped
# `target/release` build that vm_corpus and jit-diff use.
GATE_PROFILE=${GATE_PROFILE:-debug}
export GATE_PROFILE
# gate_test_env <5.5|5.4>: sets GATE_TENV (VAR=value words for `env`) and GATE_REL (extra cargo test flags) for the profile.
gate_test_env() {
  local td=target
  GATE_TENV=()
  GATE_REL=()
  if [ "$1" = 5.4 ]; then GATE_TENV+=(RUSTFLAGS="--cfg rustynail_lua54"); td=target-54; fi
  if [ "$GATE_PROFILE" != debug ]; then
    if [ "$td" = target ]; then td=target-gr; else td=target-54r; fi
    GATE_TENV+=(CARGO_PROFILE_RELEASE_LTO=off CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 CARGO_PROFILE_RELEASE_DEBUG=0)
    if [ "$GATE_PROFILE" = fast ]; then
      GATE_TENV+=(CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=true CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=true)
    else
      td=$td-noassert
    fi
    GATE_REL=(--release)
  fi
  [ "$td" != target ] && GATE_TENV+=(CARGO_TARGET_DIR="$GATE_ROOT/$td")
  return 0
}
# gate_export_test_env <version>: the same, exported (for the scripts whose every cargo call is a test build)
gate_export_test_env() {
  gate_test_env "$1"
  [ "${#GATE_TENV[@]}" -gt 0 ] && export "${GATE_TENV[@]}"
  return 0
}

# A scope with the cap; falls back to ulimit -v when systemd-run is not usable.
CAP_OK=1
systemd-run --user --scope -q -p MemoryMax=1G -p MemorySwapMax=0 true </dev/null 2>/dev/null || CAP_OK=0
if [ "$CAP_OK" = 0 ]; then echo "(systemd-run scopes unavailable: falling back to ulimit -v, no CPU sets)" >&2; fi
CPUSET_OK=0
if [ "$CAP_OK" = 1 ] && [ -n "$GATE_CPUS" ] && systemd-run --user --scope -q -p AllowedCPUs="$GATE_CPUS" true </dev/null 2>/dev/null; then CPUSET_OK=1; fi

. "$GATE_ROOT/scripts/fast-build-env.sh"   # (opt-in: GATE_FASTLINK=1 mold, sccache when installed)

# gate_cap <mem> <cpus|-> <command...>: run the command in a capped scope (cpus "-": all CPUs). Afterwards GATE_LAST_PEAK_MB is the peak
# memory of the scope (cgroup memory.peak, sampled once a second; empty when it cannot be read).
GATE_LAST_PEAK_MB=
gate_cap() {
  local mem=$1 cpus=$2; shift 2
  GATE_LAST_PEAK_MB=
  if [ "$CAP_OK" = 1 ]; then
    local unit=gate-$$-$RANDOM props=(-p MemoryMax="$mem" -p MemorySwapMax=0) rc pf sp
    if [ "$cpus" != - ] && [ "$CPUSET_OK" = 1 ]; then props+=(-p AllowedCPUs="$cpus"); fi
    pf=$GATE_DIR/.peak.$unit
    (
      cg=/sys/fs/cgroup/user.slice/user-$(id -u).slice/user@$(id -u).service/app.slice/$unit.scope
      seen=0
      last=0
      while :; do
        if [ -r "$cg/memory.peak" ]; then seen=1; last=$(cat "$cg/memory.peak" 2>/dev/null || echo "$last"); echo "$last" > "$pf"
        elif [ "$seen" = 1 ]; then break; fi
        sleep 1
      done
    ) &
    sp=$!
    systemd-run --user --scope -q --unit="$unit" "${props[@]}" "$@"
    rc=$?
    sleep 1.2
    kill "$sp" 2>/dev/null
    wait "$sp" 2>/dev/null
    [ -s "$pf" ] && GATE_LAST_PEAK_MB=$(( $(cat "$pf") / 1048576 ))
    rm -f "$pf"
    return $rc
  else
    ( ulimit -v 16000000; "$@" )
  fi
}

gate_wait_load() {
  [ "$GATE_LOAD_MAX" = 0 ] && return
  local l
  for _ in $(seq 120); do
    l=$(cut -d' ' -f1 /proc/loadavg | cut -d. -f1)
    [ "$l" -lt "$GATE_LOAD_MAX" ] && return
    sleep 5
  done
  echo "(load average still >= $GATE_LOAD_MAX after 10 minutes: starting anyway)" >&2
}

gate_selected() { # name -> 0 if the step is to be run
  if [ -n "$GATE_ONLY" ] && [[ ",$GATE_ONLY," != *",$1,"* ]]; then return 1; fi
  if [ -n "$GATE_SKIP" ] && [[ ",$GATE_SKIP," == *",$1,"* ]]; then return 1; fi
  return 0
}

gate_record() { # name result seconds note
  GATE_RESULTS+=("$(printf '%-26s %-6s %6ss  %s' "$1" "$2" "$3" "$4")")
  [ "$2" = FAIL ] && GATE_FAILED=1
  printf '%-26s %-6s %6ss  %s\n' "$1" "$2" "$3" "$4"
}

# gate_step <name> <timeout-s> <mem|-> <cpus|-> <command...>
# mem "-": GATE_MEM; cpus "-": all CPUs. The log is $GATE_DIR/<name>.log. The result is PASS or FAIL by exit status;
# a caller that classifies failures itself (KNOWN) uses gate_step_raw and gate_record.
gate_step() {
  local name=$1 to=$2 mem=$3 cpus=$4; shift 4
  gate_selected "$name" || return 0
  [ "$mem" = - ] && mem=$GATE_MEM
  gate_wait_load
  local s=$(date +%s) rc
  gate_cap "$mem" "$cpus" timeout "$to" "$@" </dev/null >"$GATE_DIR/$name.log" 2>&1
  rc=$?
  GATE_LAST_RC=$rc
  local d=$(( $(date +%s) - s ))
  local pk=${GATE_LAST_PEAK_MB:+ peak ${GATE_LAST_PEAK_MB}MB}
  if [ $rc = 0 ]; then gate_record "$name" PASS "$d" "$GATE_DIR/$name.log$pk"
  elif [ -n "${GATE_TESTS:-}" ] && gate_known_test_failures "$GATE_DIR/$name.log"; then
    gate_record "$name" KNOWN "$d" "known failing: $GATE_KNOWN_LIST $GATE_DIR/$name.log"
  else gate_record "$name" FAIL "$d" "rc=$rc $GATE_DIR/$name.log"; fi
  GATE_LAST_SECS=$d
  return $rc
}

# Known failures: scripts/gate-known.txt, lines "<class> <substring>" ('#' comments). gate_known_re <class> -> egrep pattern
gate_known_re() {
  awk -v c="$1" '$1==c { sub(/^[^ ]+ +/, ""); gsub(/[][().*+?^$|\\]/, "\\\\&"); print }' "$GATE_ROOT/scripts/gate-known.txt" | paste -sd'|' -
}

# gate_known_test_failures <cargo test log>: 0 when the log has failed tests, EVERY failed test is listed in scripts/gate-known.txt
# (class `test`, "<test binary>::<test name>"), and every failing binary has a failed test (no abort / crash / build error hides
# behind the known ones). Sets GATE_KNOWN_LIST.
gate_known_test_failures() {
  local log=$1 failed nbin nfail kre
  failed=$(awk '/^ *Running (unittests src\/lib.rs|tests\/[A-Za-z0-9_]+\.rs)/ { b = $0; sub(/.*tests\//, "", b); sub(/\.rs.*/, "", b); if ($0 ~ /unittests/) b = "lib" }
                /^test .* \.\.\. FAILED/ { print b "::" $2 }' "$log" | sort -u)
  [ -z "$failed" ] && return 1
  kre=$(gate_known_re test)
  if [ "${GATE_VERSION:-5.5}" = 5.4 ]; then   # (class test54: known on 5.4 only; a 5.5 failure of the same test is never excused)
    k4=$(gate_known_re test54)
    [ -n "$k4" ] && kre=${kre:+$kre|}$k4
  fi
  [ -z "$kre" ] && return 1
  if printf '%s\n' "$failed" | grep -qvE "^($kre)\$"; then return 1; fi
  nbin=$(printf '%s\n' "$failed" | sed 's/::.*//' | sort -u | wc -l)
  nfail=$(grep -c '^error: test failed' "$log")
  [ "$nbin" = "$nfail" ] || return 1
  GATE_KNOWN_LIST=$(printf '%s\n' "$failed" | paste -sd, -)
  return 0
}

gate_summary() {
  local total=$(( $(date +%s) - GATE_T0 ))
  echo
  echo "== $GATE_NAME summary (logs: $GATE_DIR)"
  printf '%s\n' "${GATE_RESULTS[@]}" | tee "$GATE_DIR/summary.txt"
  printf '%-26s %-6s %6ss\n' TOTAL "" "$total" | tee -a "$GATE_DIR/summary.txt"
  if [ "$GATE_FAILED" = 0 ]; then echo "$GATE_NAME: OK" | tee -a "$GATE_DIR/summary.txt"; else echo "$GATE_NAME: FAILED" | tee -a "$GATE_DIR/summary.txt"; fi
  return $GATE_FAILED
}

# Test binary of a cargo test target (built before, no run): gate_test_bin <cargo args...> -- <test name>
# prints the path of the executable of `--test <name>`.
gate_test_bin() { # <features> <test-target> [cargo target dir]
  local feat=$1 tgt=$2
  cargo test -p rn-jit "${GATE_REL[@]}" --features "$feat" --test "$tgt" --no-run </dev/null 2>&1 \
    | sed -n "s/^ *Executable tests\/$tgt\.rs (\(.*\))\$/\1/p" | head -n 1
}

# Number of CPUs of a CPU list such as "2-15" or "0,2-3,8-9" (empty: all)
gate_ncpus() {
  local list=${1:-} n=0 part a b
  [ -z "$list" ] && { nproc; return; }
  IFS=, read -ra parts <<<"$list"
  for part in "${parts[@]}"; do
    if [[ $part == *-* ]]; then a=${part%-*}; b=${part#*-}; n=$((n + b - a + 1)); else n=$((n + 1)); fi
  done
  echo "$n"
}
