#!/bin/bash
# Run the CI gate (.github/workflows/ci.yml) locally, every step memory-capped and time-limited
# (CLAUDE.md), with a summary table at the end. Logs: target/ci-local/<step>.log.
#
# usage: scripts/ci-local.sh [--full] [--only step1,step2]
#   default  fmt, clippy (5.5, 5.4), rustdoc (5.5, 5.4), cargo-deny, reference build (if missing),
#            cargo test (5.5), nextest (5.5, 5.4), official suite (5.5, 5.4), release build (default+fast)
#            plus the JIT job (jit-*): clippy/tests/official suite/jit-diff of the `jit` feature build and the
#            check that a build without the feature has the same .text as the base (scripts/jit-text-identity.sh)
#   --full   also: ASan (5.5, 5.4), ASan with the JIT, the tier-1.5 mutation matrix, Miri subset (5.5, 5.4) and Miri of the rn-jit pure
#            parts, fuzz smoke (60 s per target)
#   --only   run only the named steps (names as printed in the summary)
# Exit status: 0 if every step passed.
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
FULL=0
ONLY=""
while [ $# -gt 0 ]; do
  case "$1" in
    --full) FULL=1; shift ;;
    --only) ONLY=",$2,"; shift 2 ;;
    -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1"; exit 2 ;;
  esac
done
CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
$CAP true </dev/null 2>/dev/null || { echo "(systemd-run scopes unavailable: using ulimit -v)"; CAP=""; ulimit -v 16000000; }
LOG=target/ci-local
mkdir -p "$LOG"
RESULTS=()
FAILED=0

# step <name> <timeout-seconds> <command...>: run capped, log, record PASS/FAIL and duration
step() {
  local name=$1 to=$2; shift 2
  if [ -n "$ONLY" ] && [[ "$ONLY" != *",$name,"* ]]; then return; fi
  local s=$(date +%s)
  printf '%-22s ... ' "$name"
  if $CAP timeout "$to" "$@" </dev/null >"$LOG/$name.log" 2>&1; then
    local r=PASS
  else
    local r=FAIL; FAILED=1
  fi
  local d=$(( $(date +%s) - s ))
  echo "$r (${d}s)"
  RESULTS+=("$(printf '%-22s %-4s %5ss  %s' "$name" "$r" "$d" "$LOG/$name.log")")
}
V54=(env RUSTFLAGS="--cfg rustynail_lua54" CARGO_TARGET_DIR=target-54)

step fmt 300 cargo fmt --all --check
step clippy-5.5 1800 cargo clippy --workspace --all-targets -- -D warnings
step clippy-5.4 1800 "${V54[@]}" cargo clippy --workspace --all-targets -- -D warnings
step doc-5.5 1800 env RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
step doc-5.4 1800 "${V54[@]}" RUSTDOCFLAGS="-D warnings --cfg rustynail_lua54" cargo doc --workspace --no-deps
if command -v cargo-deny >/dev/null; then
  step deny 600 cargo deny check
else
  RESULTS+=("$(printf '%-22s %-4s' deny SKIP) (cargo install cargo-deny --locked)")
fi
if [ ! -x ref/55/build/lua ] || [ ! -x ref/54/build/lua ] || [ ! -x ref/55/build-seed0/refrun ] || [ ! -x ref/54/build-seed0/refrun ]; then
  step ref-build 1200 cargo xtask ref-build --all
fi
step test-5.5 2400 cargo test --workspace
step nextest-5.5 2400 cargo xtask test --lua-version 5.5
step nextest-5.4 2400 cargo xtask test --lua-version 5.4
step official-5.5 2400 cargo xtask official --release --lua-version 5.5
step official-5.4 2400 cargo xtask official --release --lua-version 5.4
# the official runner rewrites the status files with timings: keep the committed ones
git checkout -- tests/official-status.toml tests/official-status-5.4.toml 2>/dev/null
step release 3600 scripts/release.sh --versions 5.5,5.4 --variants default,fast --out target/ci-local/dist
# JIT job (docs/JIT.md): everything runs with the cargo feature `jit` (x86-64 Linux only)
if [ "$(uname -m)" = x86_64 ] && [ "$(uname -s)" = Linux ]; then
  JT=target/jit
  step jit-clippy 1800 cargo clippy --workspace --all-targets --features rn-cli/jit,rustynail/jit -- -D warnings
  step jit-build 3600 env CARGO_TARGET_DIR=$JT cargo build --release -p rn-cli --features jit
  step jit-test-rn-jit 2400 cargo test -p rn-jit --features jit
  step jit-test-facade 2400 cargo test -p rustynail --features jit
  step jit-test-rn-jit-5.4 2400 env RUSTFLAGS="--cfg rustynail_lua54" CARGO_TARGET_DIR=target-54 cargo test -p rn-jit -p rustynail --features rn-jit/jit,rustynail/jit
  step jit-official-always 2400 scripts/jit-official.sh $JT/release/rnlua RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=off
  step jit-official-on 2400 scripts/jit-official.sh $JT/release/rnlua RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2,20 RUSTYNAIL_JIT_TRACE=off
  step jit-diff 3000 env JITDIFF_ON=1 scripts/jit-diff.sh 1 500 $JT/release/rnlua
  # F9 (D3): the shipped default configuration (traces on, J4 regions as fallback owner): nothing pinned
  step jit-official-default 2400 scripts/jit-official.sh $JT/release/rnlua
  step jit-diff-default 3000 env JITDIFF_ENTERS=1 JITDIFF_MODES="off trdef trdefon" scripts/jit-diff.sh 1 300 $JT/release/rnlua
  step jit-text-identity 3600 scripts/jit-text-identity.sh
  # tier 1.5 verification (docs/JIT_TIER15.md section 22)
  step jit-t15-always 2400 env RUSTYNAIL_JIT_T15=always cargo test -p rn-jit --features jit
  step jit-t15-stress 3000 cargo test -p rn-jit --features jit --test t15_stress
  step jit-t15-clippy 1800 bash -c 'cargo clippy -p rn-jit --features jit,t15_mutants --all-targets -- -D warnings && cargo clippy -p rn-jit --features t15_oracle --all-targets -- -D warnings'
  step jit-t15-official-shadow 2400 scripts/jit-official.sh $JT/release/rnlua RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+shadow RUSTYNAIL_JIT_TRACE=off
  step jit-t15-official-storm 2400 scripts/jit-official.sh $JT/release/rnlua RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force,+exitstorm=7 RUSTYNAIL_JIT_TRACE=off
  step jit-t15-diff 3000 env JITDIFF_ON=1 JITDIFF_ENTERS=1 RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force \
    JITDIFF_MODES="off always t15 t15flush t15rand t15verify t15shadow t15shadowrand t15storm t15stormshadow t15stormprof" \
    scripts/jit-diff.sh 1 300 $JT/release/rnlua
  step jit-dump-identity 3000 scripts/jit-dump-identity.sh
  # J5 trace compiler (docs/JIT_TRACE.md 9.8): the tr_* test binaries, the trace jit-diff modes (with the NOTRACES /
  # FEWTRACES counter assertions) on seeds 1-200 and the instruction-count identity
  TT=target/jit-trace
  step trace-build 3600 env CARGO_TARGET_DIR=$TT cargo build --release -p rn-cli --features trace
  step trace-tests 5400 bash -c 'cargo test -p rn-jit --features trace --no-fail-fast $(for f in crates/rn-jit/tests/tr_*.rs; do printf -- "--test %s " "$(basename "$f" .rs)"; done)'
  step trace-identity 5400 cargo test -p rn-jit --features trace,t15_oracle --test tr_identity --test tr_oracle --release
  step trace-official 2400 scripts/jit-official.sh $TT/release/rnlua RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always
  step trace-diff 3600 env JITDIFF_ENTERS=1 JITDIFF_MODES="off tr tron trstack trstorm tremu trrand" \
    scripts/jit-diff.sh 1 200 $TT/release/rnlua
  step trace-diff-hooks 3600 env JITDIFF_ENTERS=1 JITDIFF_HOOK=all JITDIFF_MODES="off tr tron" \
    scripts/jit-diff.sh 1 100 $TT/release/rnlua
else
  RESULTS+=("$(printf '%-22s %-4s' jit SKIP) (x86-64 Linux only)")
fi
if [ $FULL = 1 ]; then
  step asan-jit-5.5 7200 env LUA_VERSION=5.5 scripts/asan-jit.sh /tmp/rustynail-asan-jit
  step miri-jit 7200 scripts/miri.sh jit
  step miri-jit-trace 7200 scripts/miri.sh jit-trace
  step fuzz-trace 3600 bash -c 'cargo build -p rn-cli && fuzz/make_trace_seeds.sh && cd fuzz && export CARGO_TARGET_DIR=/tmp/jitfuzz-target &&
      cargo +nightly fuzz build --features trace &&
      for t in trace_ir jit_diff run_source; do
        echo "== $t"; /tmp/jitfuzz-target/x86_64-unknown-linux-gnu/release/$t corpus-trace/$t -max_total_time=60 -rss_limit_mb=2500 -timeout=30 || exit 1
      done'
  step t15-mutants 14400 scripts/t15-mutants.sh
  step asan-5.5 5400 env LUA_VERSION=5.5 scripts/asan.sh /tmp/rustynail-asan
  step asan-5.4 5400 env LUA_VERSION=5.4 scripts/asan.sh /tmp/rustynail-asan-54
  MIRI_TESTS="string:: heap:: tests_table tests_gc value:: alloc::"
  # shellcheck disable=SC2086
  step miri-5.5 7200 env CARGO_TARGET_DIR=target/miri cargo +nightly miri test -p rn-core --lib -- $MIRI_TESTS
  # shellcheck disable=SC2086
  step miri-5.4 7200 env RUSTFLAGS="--cfg rustynail_lua54" CARGO_TARGET_DIR=target-54/miri \
    cargo +nightly miri test -p rn-core --lib -- $MIRI_TESTS
  step fuzz-seeds 1200 bash -c 'cargo build -p rn-cli && fuzz/make_seeds.sh'
  step fuzz-smoke 3600 bash -c 'cd fuzz && for t in $(cargo +nightly fuzz list); do
      echo "== $t"; cargo +nightly fuzz run "$t" -- -max_total_time=60 -rss_limit_mb=2500 -timeout=30 || exit 1
    done'
fi

echo
echo "== summary"
printf '%s\n' "${RESULTS[@]}"
[ $FAILED = 0 ] && echo "CI-LOCAL OK" || { echo "CI-LOCAL FAILED"; exit 1; }
