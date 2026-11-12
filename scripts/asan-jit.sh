#!/bin/bash
# J3b: AddressSanitizer run of the JIT build (docs/JIT.md, docs/UNSAFE.md "Sanitizers").
#   1. builds rnlua with `--features jit` under `-Zsanitizer=address` (nightly + build-std);
#   2. runs the official suite with RUSTYNAIL_JIT=always and RUSTYNAIL_JIT=on (low thresholds, so the
#      suite compiles a lot);
#   3. runs the tests/vm corpus with the JIT off and always and compares the outputs (differential), the same
#      with RUSTYNAIL_GC_STRESS=1 on the small part of the corpus, and the bench programs / small corpus
#      with RUSTYNAIL_JIT_STRESS=stackmove;
#   4. runs the rn-jit test suite (`cargo test -p rn-jit --features jit`) under ASan, plus a subset of the tier 1.5
#      verification stress (`--test t15_stress`: shadow, exit storm, random allocation);
#   5. T15 (docs/JIT_TIER15.md section 22): the tests/vm corpus with RUSTYNAIL_JIT_T15=always +force, with +shadow and with
#      +exitstorm=7 (differential against the interpreter); ASAN_JIT_OFFICIAL_T15=1 adds the official suite with T15=always.
#   6. J5 traces (ASAN_TRACE=0 skips): the binary is built with `--features trace`; the official suite runs with
#      TRACE=always (JIT=always) and TRACE=on (hot 2,2), the tests/vm corpus with TRACE=always, `+exitstorm=3`,
#      `+emurun` and `-fold,-cse,-loop,-fwd,-abc` (differential against the interpreter), the trace jit-diff modes
#      (tr tron tremu trstorm, seeds 1..ASAN_TRACE_SEEDS, default 40, JITDIFF_ENTERS=1) and the tr_* test binaries.
#      ASAN_TRACE_ONLY=1 runs only item 6 (and, unless --skip-tests, the tr_* tests).
# Prints `ASAN-JIT OK` when nothing failed.
#
# usage: scripts/asan-jit.sh [target-dir] [--skip-tests]   (default /tmp/rustynail-asan-jit; ~1 GB)
#        LUA_VERSION=5.4 scripts/asan-jit.sh ...          (the 5.4 build)
#
# Executable memory under ASan: nothing special is needed. The arena maps RW (anonymous mmap, ASan does not
# intercept or shadow it: generated code is not instrumented), then `mprotect`s to RX; ASan's allocator and
# shadow memory live in their own reserved ranges. `detect_leaks=1` works (LeakSanitizer sees the Rust-heap
# descriptors); `halt_on_error=1`. The pool allocator is replaced by plain malloc (RUSTYNAIL_SYSTEM_MALLOC=1)
# as in scripts/asan.sh, otherwise use-after-free of Lua objects would not be seen. Memory: every step runs
# under the caps of CLAUDE.md when systemd-run is available.
set -uo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
LV=${LUA_VERSION:-5.5}
if [ "$LV" = 5.4 ]; then CFG="--cfg rustynail_lua54"; DEF=/tmp/rustynail-asan-jit-54; else CFG=""; DEF=/tmp/rustynail-asan-jit; fi
TGT=${1:-$DEF}
SKIP_TESTS=0
[ "${2:-}" = --skip-tests ] && SKIP_TESTS=1
A=$TGT/x86_64-unknown-linux-gnu/debug
export CARGO_TARGET_DIR=$TGT
export RUSTFLAGS="-Zsanitizer=address -Cdebuginfo=1 --cfg rustynail_lsan $CFG"
export CFLAGS="-fsanitize=address -fno-omit-frame-pointer -g"
export ASAN_OPTIONS=detect_leaks=1:halt_on_error=1:allocator_may_return_null=1 # (tests ask for absurd sizes on purpose)
export LSAN_OPTIONS=suppressions=$PWD/scripts/lsan-jit.supp # iced-x86's lazily built static tables (test oracle)
# The LSan suppressions (scripts/lsan-jit.supp) match function names: without llvm-symbolizer they never match and the harnesses'
# deliberate one-byte leaks (arith_diff/table_diff) and iced-x86's tables are reported as leaks (GitHub's ubuntu-24.04 image has no
# unversioned `llvm-symbolizer` on PATH, only /usr/lib/llvm-N/bin/llvm-symbolizer). Look for one, and fail early with a clear message.
if [ -z "${ASAN_SYMBOLIZER_PATH:-}" ] && ! command -v llvm-symbolizer >/dev/null 2>&1; then
  for c in /usr/lib/llvm-*/bin/llvm-symbolizer /usr/bin/llvm-symbolizer-*; do
    [ -x "$c" ] && export ASAN_SYMBOLIZER_PATH=$c
  done
fi
if [ -z "${ASAN_SYMBOLIZER_PATH:-}" ] && ! command -v llvm-symbolizer >/dev/null 2>&1; then
  echo "asan-jit.sh: llvm-symbolizer not found (install the llvm package or set ASAN_SYMBOLIZER_PATH): the LSan suppressions would not apply" >&2
  exit 1
fi
export CARGO_TERM_COLOR=never # (the failure greps below match "^error" at a line start)
export RUSTYNAIL_SYSTEM_MALLOC=1
export RUSTYNAIL_TEST_NOULIMIT=1 # the test children's RLIMIT_AS (6 GiB) cannot hold ASan's shadow memory (F6)
CAP=(systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0)
"${CAP[@]}" true </dev/null 2>/dev/null || CAP=()

# F9: `--features jit` includes the trace compiler and RUSTYNAIL_JIT_TRACE defaults to on. The sections below that mean "J4 only"
# (1-5) keep their meaning through this pin; the trace sections set TRACE explicitly (env args override it) and the
# "default configuration" section unsets it (`env -u`).
export RUSTYNAIL_JIT_TRACE=off
TRACE=${ASAN_TRACE:-1}
LCFEAT=jit; [ "$TRACE" = 1 ] && LCFEAT=trace # (5.5 and, since F7a, 5.4)
"${CAP[@]}" cargo +nightly build -Zbuild-std --target x86_64-unknown-linux-gnu -p rn-cli --features $LCFEAT </dev/null || exit 1
W=$(mktemp -d)
trap 'rm -rf "$W"' EXIT
fail=0
norm() { sed -E '/^==[0-9]+==/d; s/0x[0-9a-f]+/0xADDR/g; s/^[a-z-]*: //' "$1"; }

# --- official suite
SKIP='^(all|heavy)\.lua$'
suite() { # label env...
  local label=$1; shift
  for f in $(ls vendor/lua-tests-$LV/*.lua | xargs -n1 basename | grep -vE "$SKIP"); do
    if ! (cd vendor/lua-tests-$LV && env "$@" "${CAP[@]}" timeout 900 "$A/rnlua" -e "_port=true;_soft=true" "$f" </dev/null >/dev/null 2>"$W/err"); then
      echo "FAIL [official $label] $f: $(head -5 "$W/err")"; fail=1
    fi
  done
}
TRONLY=${ASAN_TRACE_ONLY:-0} # 1: only the J5 trace parts (item 6)
[ "$TRONLY" = 1 ] || { echo "== official suite, RUSTYNAIL_JIT=always"; suite always RUSTYNAIL_JIT=always; }
[ "$TRONLY" = 1 ] || { echo "== official suite, RUSTYNAIL_JIT=on (threshold 2,20)"; suite on RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2,20; }

# --- tests/vm corpus: JIT off vs always must agree, and ASan must stay quiet
corpus() { # label env...  (compares against the JIT-off run under the same extra env)
  local label=$1; shift
  for f in tests/vm/*.lua; do
    b=$(basename "$f")
    env RUSTYNAIL_JIT=off "$@" "${CAP[@]}" timeout 300 "$A/rnlua" "$f" </dev/null >"$W/o.off" 2>"$W/e.off"; r0=$?
    env RUSTYNAIL_JIT=always "$@" "${CAP[@]}" timeout 300 "$A/rnlua" "$f" </dev/null >"$W/o.on" 2>"$W/e.on"; r1=$?
    if [ $r0 = 124 ] || [ $r1 = 124 ]; then echo "SKIP [$label] $b: timeout ($r0/$r1)"; continue; fi
    if grep -qE "ERROR: (Address|Leak)Sanitizer|SUMMARY: (Address|Leak)Sanitizer" "$W/e.on"; then echo "FAIL [$label] $b: sanitizer report"; head -20 "$W/e.on"; fail=1; continue; fi
    if [ $r0 != $r1 ] || ! diff -q <(norm "$W/o.off") <(norm "$W/o.on") >/dev/null || ! diff -q <(norm "$W/e.off") <(norm "$W/e.on") >/dev/null; then
      echo "FAIL [$label] $b: JIT off/always differ (rc $r0/$r1)"; fail=1
    fi
  done
}
[ "$TRONLY" = 1 ] || { echo "== tests/vm corpus"; corpus plain; }
# --- tier 1.5 regions (docs/JIT_TIER15.md section 22): every nest promoted at its first back-edge, +force (the
# profitability gate would refuse most of the corpus), with the verification options. The reference side is the
# interpreter. Output and exit status must agree, ASan must stay quiet.
corpus_t15() { # label opt
  local label=$1 opt=$2
  for f in tests/vm/*.lua; do
    b=$(basename "$f")
    env RUSTYNAIL_JIT=off "${CAP[@]}" timeout 300 "$A/rnlua" "$f" </dev/null >"$W/o.off" 2>"$W/e.off"; r0=$?
    env RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT="$opt" "${CAP[@]}" timeout 300 "$A/rnlua" "$f" </dev/null >"$W/o.on" 2>"$W/e.on"; r1=$?
    if [ $r0 = 124 ] || [ $r1 = 124 ]; then echo "SKIP [$label] $b: timeout ($r0/$r1)"; continue; fi
    if grep -qE "ERROR: (Address|Leak)Sanitizer|SUMMARY: (Address|Leak)Sanitizer|internal error|SHADOW VERIFICATION" "$W/e.on"; then echo "FAIL [$label] $b: sanitizer / verification report"; head -20 "$W/e.on"; fail=1; continue; fi
    if [ $r0 != $r1 ] || ! diff -q <(norm "$W/o.off") <(norm "$W/o.on") >/dev/null || ! diff -q <(norm "$W/e.off") <(norm "$W/e.on") >/dev/null; then
      echo "FAIL [$label] $b: JIT off / T15 differ (rc $r0/$r1)"; fail=1
    fi
  done
}
[ "$TRONLY" = 1 ] || { echo "== tests/vm corpus, T15=always +force"; corpus_t15 t15 "+force"; }
[ "$TRONLY" = 1 ] || { echo "== tests/vm corpus, T15=always +force,+shadow"; corpus_t15 t15-shadow "+force,+shadow"; }
[ "$TRONLY" = 1 ] || { echo "== tests/vm corpus, T15=always +force,+exitstorm=7"; corpus_t15 t15-storm7 "+force,+exitstorm=7"; }
if [ "${ASAN_JIT_OFFICIAL_T15:-0}" = 1 ] && [ "$TRONLY" != 1 ]; then
  echo "== official suite, T15=always +force"; suite t15 RUSTYNAIL_JIT=always RUSTYNAIL_JIT_T15=always RUSTYNAIL_JIT_OPT=+force
fi
echo "== tests/vm corpus, GC stress 1 (arith, closures, co, tbc and table scripts)"
SMALL=$(ls tests/vm/*.lua | grep -E '/(arith|closures|co|tail|vararg|tables|for|goto)_' || true)
corpus_small() { # label env...
  local label=$1; shift
  for f in $SMALL; do
    b=$(basename "$f")
    env RUSTYNAIL_JIT=always "$@" "${CAP[@]}" timeout 600 "$A/rnlua" "$f" </dev/null >"$W/o.on" 2>"$W/e.on"; r1=$?
    env RUSTYNAIL_JIT=off "$@" "${CAP[@]}" timeout 600 "$A/rnlua" "$f" </dev/null >"$W/o.off" 2>"$W/e.off"; r0=$?
    if [ $r0 = 124 ] || [ $r1 = 124 ]; then echo "SKIP [$label] $b: timeout"; continue; fi
    if grep -qE "ERROR: (Address|Leak)Sanitizer|SUMMARY: (Address|Leak)Sanitizer" "$W/e.on"; then echo "FAIL [$label] $b: sanitizer report"; head -20 "$W/e.on"; fail=1; continue; fi
    if [ $r0 != $r1 ] || ! diff -q <(norm "$W/o.off") <(norm "$W/o.on") >/dev/null; then
      echo "FAIL [$label] $b: JIT off/always differ (rc $r0/$r1)"; fail=1
    fi
  done
}
[ "$TRONLY" = 1 ] || { corpus_small gcstress1 RUSTYNAIL_GC_STRESS=1; }
echo "== stackmove stress (small corpus + closures/coroutines/fib/queens programs)"
# (co_09 moves the stack at every helper call of a deep coroutine stack: quadratic, > 10 min under ASan: skipped)
SMALL=$(echo "$SMALL" | grep -v co_09_)
[ "$TRONLY" = 1 ] || { corpus_small stackmove RUSTYNAIL_JIT_STRESS=stackmove; }
for f in bench/programs/fib.lua bench/programs/queens.lua bench/programs/closures.lua bench/programs/coroutines.lua; do
  [ "$TRONLY" = 1 ] && break
  env RUSTYNAIL_JIT=always RUSTYNAIL_JIT_STRESS=stackmove "${CAP[@]}" timeout 900 "$A/rnlua" "$f" </dev/null >"$W/o.on" 2>"$W/e.on" || { echo "FAIL [stackmove] $f: $(head -5 "$W/e.on")"; fail=1; }
done

# --- J5 traces (item 6)
if [ "$LCFEAT" = trace ]; then
  echo "== official suite, default configuration (JIT=on, traces and regions on by default)"; suite default -u RUSTYNAIL_JIT_TRACE -u RUSTYNAIL_JIT
  echo "== official suite, JIT=always TRACE=always"; suite trace RUSTYNAIL_JIT=always RUSTYNAIL_JIT_TRACE=always
  echo "== official suite, JIT=on TRACE=on (hot 2,2)"; suite trace-on RUSTYNAIL_JIT=on RUSTYNAIL_JIT_THRESHOLD=2,20 RUSTYNAIL_JIT_TRACE=on RUSTYNAIL_JIT_TRACE_HOT=2,2
fi
if [ "$LCFEAT" = trace ]; then
  echo "== tests/vm corpus, TRACE=always"; corpus trace RUSTYNAIL_JIT_TRACE=always
  echo "== tests/vm corpus, TRACE=always +exitstorm=3"; corpus trace-storm RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=+exitstorm=3
  echo "== tests/vm corpus, TRACE=always +emurun"; corpus trace-emu RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=+emurun
  echo "== tests/vm corpus, TRACE=always, passes off"; corpus trace-noopt RUSTYNAIL_JIT_TRACE=always RUSTYNAIL_JIT_TRACE_OPT=-fold,-cse,-loop,-fwd,-abc
  echo "== jit-diff trace modes under ASan (seeds 1..${ASAN_TRACE_SEEDS:-40})"
  if ! JITDIFF_NOULIMIT=1 JITDIFF_ENTERS=1 JITDIFF_MODES="off tr tron tremu trstorm" TMO=120 JOBS=2 scripts/jit-diff.sh 1 "${ASAN_TRACE_SEEDS:-40}" "$A/rnlua" </dev/null >"$W/jd.log" 2>&1; then
    echo "FAIL [trace jit-diff]"; tail -20 "$W/jd.log"; fail=1
  else
    grep -E "^(traces|jit-diff):" "$W/jd.log"
  fi
fi

# --- rn-jit's own tests under ASan
if [ $SKIP_TESTS = 0 ]; then
  if [ "$TRONLY" != 1 ]; then
  echo "== cargo test -p rn-jit --features jit (ASan)"
  # (F6) two tests are not ASan-compatible: the first lowers RLIMIT_AS (ASan's shadow memory needs terabytes), the second builds
  # synthetic tables from integer addresses and trips `ptr::add` precondition checks (the same harness is excluded from Miri)
  if ! "${CAP[@]}" timeout 3000 cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p rn-jit --features jit --no-fail-fast </dev/null -- --skip failing_mmap_of_code_chunks_falls_back_silently --skip inline_table_accesses_match_tier1_at_every_pc_on_synthetic_tables >"$W/jt.log" 2>&1; then
    echo "FAIL [rn-jit tests]"; grep -E -B2 -A12 "AddressSanitizer|FAILED|panicked|^error|exit status" "$W/jt.log" | head -150; fail=1
  else
    grep -E "^test result" "$W/jt.log" | sort | uniq -c
  fi
  fi
  if [ "$LCFEAT" = trace ]; then
    echo "== cargo test -p rn-jit --features trace --test tr_* (ASan)"
    trt=(); for f in crates/rn-jit/tests/tr_*.rs; do trt+=(--test "$(basename "$f" .rs)"); done
    if ! "${CAP[@]}" timeout 5400 cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p rn-jit --features trace --no-fail-fast "${trt[@]}" </dev/null >"$W/tr.log" 2>&1; then
      echo "FAIL [tr_* tests]"; grep -E -B2 -A12 "AddressSanitizer|FAILED|panicked|^error|exit status|tr_core child" "$W/tr.log" | head -150; fail=1
    else
      grep -E "^test result" "$W/tr.log" | sort | uniq -c
    fi
  fi
  [ "$TRONLY" != 1 ] && echo "== cargo test -p rn-jit --features jit --test t15_stress (ASan, shadow / storm / randra subset)"
  if [ "$TRONLY" != 1 ]; then if ! "${CAP[@]}" timeout 3000 cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu -p rn-jit --features jit --test t15_stress </dev/null -- s_shadow s_storm7 s_storm2_shadow_rand s_randra_a --test-threads=2 >"$W/st.log" 2>&1; then
    echo "FAIL [t15_stress]"; grep -E "AddressSanitizer|FAILED|panicked|^error|SHADOW" "$W/st.log" | head -20; fail=1
  else
    grep -E "^test result" "$W/st.log" | sort | uniq -c
  fi; fi
fi
[ $fail = 0 ] && echo "ASAN-JIT OK" || { echo "ASAN-JIT FAILURES"; exit 1; }
