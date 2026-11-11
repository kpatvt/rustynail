#!/usr/bin/env bash
# Miri runs (nightly `cargo miri` + cargo-nextest, one process per test so tests run in parallel and
# leaks are checked per test). See docs/UNSAFE.md "Miri" for what each suite covers and the results.
#
#   scripts/miri.sh [--lua-version 5.5|5.4] [--borrows sb|tb] [--jobs N] [--logs DIR] [SUITE...]
#
# Suites (default: all):
#   core-lib      rn-core unit tests (heap, strings, tables, GC incremental/generational, State, stub VM)
#   core-it       rn-core integration tests (table_diff runs its 400 scripts on our side only under Miri)
#   compiler      rn-compiler unit + reference-free integration tests
#   interp        tests/vm scripts through the real compiler + interpreter (inc, gen, pool allocator); the
#                 other rn-std tests need libc (io streams, snprintf) or reference processes: not Miri-runnable
#   interp-s1     interp with RUSTYNAIL_GC_STRESS=1 (full collection at every safepoint)
#   interp-s2     interp with RUSTYNAIL_GC_STRESS=2 (one step / young collection per safepoint)
#   jit-trace     the J5 IR core (`--features trace`, docs/JIT_TRACE.md 9.1): `tr_ir` and `tr_passes` (builder, fold, CSE, the
#                 reference IR interpreter, random-IR differential tests with Miri-sized case counts), the recorder replay (`tr_record`,
#                 `miri_*`: the recorded IR against the real interpreter), the trace assembler in the emulator against the IR
#                 interpreter (`tr_asm` `emu_*`/`hand_*`, Miri-sized case counts), the `tr_mutants` registry-vs-table check (the child-process runner is `cfg(not(miri))`) and the `tr::` unit tests
#   jit           the pure-Rust parts of rn-jit (`--features jit`): the tier-1.5 region analysis (`t15_analysis`), the register
#                 allocator (`t15::alloc` unit tests), the region code generator, stubs and exits run in the emulator on
#                 synthetic frames (`t15_core_emu`: the float and exit-storm tests; the others hit pre-existing Miri failures
#                 (integer-pointer casts in the synthetic-table harness) or take over an hour, see docs/JIT_TIER15.md 22.5),
#                 the x86-64 encoder, the micro-op IR emulator
#                 (`emu_diff`: templates driven by hand-assembled bytecode on synthetic frames, no machine code
#                 executed; `table_diff` is not Miri-runnable: its emulator dereferences GC object addresses that it
#                 read back as integers from Value payloads, which Miri reports as provenance-less pointers), lowering, resume tables, the entry monitor. Tests
#                 that map or run executable memory are `cfg(miri)`-ignored; the encoder tests that use the iced-x86 oracle
#                 are excluded by filter (Miri reports Stacked Borrows UB inside iced-x86 1.21, a third-party dev
#                 dependency; and the emulator turns register integers into pointers, so strict provenance is off).
# Every run is under a memory cap and a timeout; logs go to target/miri-logs/<ver>-<borrows>-<suite>.log.
set -uo pipefail
cd "$(dirname "$0")/.."
ver=5.5 borrows=sb jobs=4 logs="$PWD/target/miri-logs"
suites=()
while [ $# -gt 0 ]; do
  case "$1" in
    --lua-version) ver=$2; shift 2 ;;
    --borrows) borrows=$2; shift 2 ;;
    --jobs) jobs=$2; shift 2 ;;
    --logs) logs=$2; shift 2 ;;
    -h|--help) sed -n 2,22p "$0"; exit 0 ;;
    *) suites+=("$1"); shift ;;
  esac
done
[ ${#suites[@]} -eq 0 ] && suites=(core-lib core-it compiler interp interp-s1 interp-s2 jit jit-trace)
mkdir -p "$logs"
if [ "$ver" = 5.4 ]; then
  export RUSTFLAGS="${RUSTFLAGS:-} --cfg rustynail_lua54"
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target-54}"
fi
base=""
[ "$borrows" = tb ] && base="-Zmiri-tree-borrows"
cap=(systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0)
command -v systemd-run >/dev/null || cap=()

run() { # name extra-miriflags env... -- cargo args...
  local name=$1 flags=$2; shift 2
  local envs=()
  while [ "$1" != -- ]; do envs+=("$1"); shift; done
  shift
  local log="$logs/${ver/./}-$borrows-$name.log"
  echo "== $name ($ver, $borrows): $log"
  local t0=$SECONDS
  env "${envs[@]}" MIRIFLAGS="$base $flags" "${cap[@]}" timeout 14400 \
    cargo +nightly miri nextest run --no-fail-fast -j "$jobs" "$@" </dev/null >"$log" 2>&1
  local st=$?
  echo "   exit $st in $((SECONDS - t0))s: $(grep -E '^ +Summary' "$log" | tail -1)"
  grep -E "Undefined Behavior|memory leaked|unsupported operation" "$log" | sort | uniq -c | head
  return $st
}

fail=0
for s in "${suites[@]}"; do
  case "$s" in
    core-lib) run core-lib "" -- -p rn-core --lib || fail=1 ;;
    core-it) run core-it "" -- -p rn-core --tests -E 'kind(test)' || fail=1 ;;
    compiler) run compiler "" -- -p rn-compiler || fail=1 ;;
    interp) run interp "-Zmiri-disable-isolation -Zmiri-deterministic-floats" -- -p rn-std --test interp_scripts || fail=1 ;;
    interp-s1) run interp-s1 "-Zmiri-disable-isolation -Zmiri-deterministic-floats -Zmiri-env-forward=RUSTYNAIL_GC_STRESS" RUSTYNAIL_GC_STRESS=1 -- \
                 -p rn-std --test interp_scripts -E 'test(/^(inc|gen)_/)' || fail=1 ;;
    interp-s2) run interp-s2 "-Zmiri-disable-isolation -Zmiri-deterministic-floats -Zmiri-env-forward=RUSTYNAIL_GC_STRESS" RUSTYNAIL_GC_STRESS=2 -- \
                 -p rn-std --test interp_scripts -E 'test(/^(inc|gen)_/)' || fail=1 ;;
    jit) run jit "-Zmiri-disable-isolation -Zmiri-deterministic-floats" -- \
                 -p rn-jit --features jit --lib --test emu_diff --test emu_ops_diff --test t15_analysis --test t15_core_emu \
                 -E 'binary(emu_diff) | binary(emu_ops_diff) | binary(t15_analysis) | (binary(t15_core_emu) & !test(/region_code_is_generated_without|failure_pads|inline_table_accesses|intrinsic_calls|regions_with_helper_calls|regions_match_tier1_emulated_and_native/)) | test(/mon_tests|bad_scale|foreign_labels|known_good|label_errors|rel32|rip_operand|rsp_index|t15::alloc|t15::runtime::tests/)' || fail=1 ;;
    jit-trace) run jit-trace "-Zmiri-disable-isolation -Zmiri-deterministic-floats" -- \
                 -p rn-jit --features trace --lib --test tr_ir --test tr_passes --test tr_loop --test tr_ra --test tr_record --test tr_record_calls --test tr_exit --test tr_asm --test tr_mutants \
                 -E 'binary(tr_ir) | binary(tr_passes) | (binary(tr_loop) & !test(/^recorded::recorded_loops/)) | binary(tr_ra) | binary(tr_exit) | binary(tr_mutants) | (binary(tr_asm) & test(/^(emu_|hand_)/)) | (binary(tr_record) & test(/^miri_/)) | (binary(tr_record_calls) & test(/^miri_/)) | test(/^tr::/)' || fail=1 ;;
    *) echo "unknown suite $s" >&2; exit 2 ;;
  esac
done
exit $fail
