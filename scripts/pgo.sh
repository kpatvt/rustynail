#!/bin/bash
# Profile-guided release build of rnlua (the shipped configuration: scripts/release-build.sh builds it by default;
# `--no-pgo` there selects the plain `cargo build --release`). All rustc/cc steps remap build-machine paths
# (scripts/release-env.sh); strip the result before shipping (release-build.sh / release.sh do).
#   1. builds an instrumented rnlua (-Cprofile-generate) in <target>/pgo-gen;
#   2. trains it with scripts/train.sh (the one list of training programs: the benchmark sets awfy, apps, programs,
#      lib, jit-trace, real plus bench/train incl. GC- and string-heavy programs, bench/micro, tests/vm, part of the
#      official suite; PGO_HOLDOUT=1 leaves the benchmark sets out so that they can serve as a hold-out);
#   3. rebuilds with -Cprofile-use into <target>/pgo (binary: <target>/pgo/release/rnlua), with the
#      absolute dispatch jump table of scripts/build-fast.sh (relocation-model=static on the binary crate
#      only; PGO_PIE=1 keeps a PIE).
# <target> is target for Lua 5.5 and target-54 for LUA_VERSION=5.4.
# Env: PGO_EMIT_RELOCS=1 links with --emit-relocs so that scripts/bolt.sh can post-process the result
#      (BOLT_PGO=1 scripts/bolt.sh does this); PGO_REPEAT=n runs the training mix n times (default 1).
#      PGO_FEATURES=jit builds the JIT + trace compiler binary (training then also exercises the compiler and
#      the trace recorder); PGO_OUT=<dir> overrides the output/data root (default <target>); PGO_RUSTFLAGS adds flags
#      (e.g. -Ctarget-cpu=x86-64-v3); PGO_STATIC=0 keeps the PIE (same as PGO_PIE=1);
#      the default is PIE when PGO_FEATURES is set (P-I: the static relocation model made trace programs such as
#      oop_override 13% slower at equal instruction counts) and static otherwise (PGO_STATIC=1 forces it).
# Needs llvm-profdata matching rustc's LLVM (rustup component llvm-tools, see rust-toolchain.toml).
# usage: [LUA_VERSION=5.4] scripts/pgo.sh      (every step is memory-capped and time-limited, see CLAUDE.md)
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
. scripts/release-env.sh   # RN_REMAP / RN_CFLAGS: no build-machine paths in the binary
case "${LUA_VERSION:-5.5}" in
  5.5) T=target; VFLAGS="" ;;
  5.4) T=target-54; VFLAGS="--cfg rustynail_lua54" ;;
  *) echo "LUA_VERSION must be 5.5 or 5.4"; exit 2 ;;
esac
# llvm-profdata must match rustc's LLVM: $LLVM_PROFDATA, else the rustup component llvm-tools-preview (listed in
# rust-toolchain.toml; installed on demand if rustup is there), else a `llvm-profdata` in PATH (a version mismatch
# makes the merge fail with "unsupported instrumentation profile format version").
SYSROOT=$(rustc --print sysroot)
find_pd() {
  if [ -n "${LLVM_PROFDATA:-}" ] && [ -x "$LLVM_PROFDATA" ]; then echo "$LLVM_PROFDATA"; return; fi
  ls "$SYSROOT"/lib/rustlib/*/bin/llvm-profdata 2>/dev/null | head -1
}
PD=$(find_pd)
if [ ! -x "$PD" ] && command -v rustup >/dev/null 2>&1; then
  echo "llvm-profdata not found: rustup component add llvm-tools-preview" >&2
  rustup component add llvm-tools-preview </dev/null >&2 || true
  PD=$(find_pd)
fi
[ -x "$PD" ] || PD=$(command -v llvm-profdata || true)
[ -x "$PD" ] || { echo "llvm-profdata not found (rustup component add llvm-tools-preview, or set LLVM_PROFDATA)" >&2; exit 1; }
OUT=$(realpath -m "${PGO_OUT:-$T}")
rn_remap_target "$OUT"; export CFLAGS="$RN_CFLAGS"
# the JIT is a default feature of rn-cli since BUNDLE W: select the feature set explicitly (unset = no JIT)
FEAT=(--no-default-features)
[ -n "${PGO_FEATURES:-}" ] && FEAT=(--no-default-features --features "$PGO_FEATURES")
DATA=$OUT/pgo-data
rm -rf "$DATA"; mkdir -p "$DATA"
CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
$CAP true </dev/null 2>/dev/null || CAP=""   # no user systemd (e.g. CI runners): uncapped
RUSTFLAGS="$VFLAGS $RN_REMAP ${PGO_RUSTFLAGS:-} -Cprofile-generate=$DATA" CARGO_TARGET_DIR=$OUT/pgo-gen $CAP timeout 900 \
  cargo build --release -p rn-cli "${FEAT[@]}" </dev/null
# One raw profile per process (%p) and an OFFLINE merge. The online merge of %m (one continuously merged file per binary
# signature) loses value profiles (indirect-call targets and memop sizes: only ~15% / ~49% overlap with an offline merge,
# deterministic), which costs ~0.8% cycles on all code. The price is a few thousand raw files (~270 MB), so they live
# in $DATA/raw (under the output dir, not /tmp), are listed in a file for `llvm-profdata merge --input-files`, and are
# deleted right after the merge.
RAW=$DATA/raw
mkdir -p "$RAW"
LLVM_PROFILE_FILE="$RAW/p%p.profraw" $CAP timeout 3600 scripts/train.sh "$OUT/pgo-gen/release/rnlua" "${PGO_REPEAT:-1}"
find "$RAW" -name '*.profraw' -size +0 > "$DATA/profraw.list"
[ -s "$DATA/profraw.list" ] || { echo "training produced no profile data in $RAW" >&2; exit 1; }
echo "merging $(wc -l < "$DATA/profraw.list") raw profiles with $PD" >&2
$CAP timeout 1800 "$PD" merge -o "$DATA/merged.profdata" --input-files="$DATA/profraw.list" </dev/null
rm -rf "$RAW" "$DATA/profraw.list" "$DATA"/*.profraw   # (the last: stray files of the instrumented build itself)
LINK=()
[ "${PGO_EMIT_RELOCS:-0}" = 1 ] && LINK=(-C link-arg=-Wl,--emit-relocs)
if [ "${PGO_PIE:-0}" = 1 ] || [ "${PGO_STATIC:-$([ -n "${PGO_FEATURES:-}" ] && echo 0 || echo 1)}" = 0 ]; then
  RUSTFLAGS="$VFLAGS $RN_REMAP ${PGO_RUSTFLAGS:-} -Cprofile-use=$DATA/merged.profdata" CARGO_TARGET_DIR=$OUT/pgo $CAP timeout 900 \
    cargo rustc --release -p rn-cli "${FEAT[@]}" --bin rnlua -- "${LINK[@]}" </dev/null
else
  RUSTFLAGS="$VFLAGS $RN_REMAP ${PGO_RUSTFLAGS:-} -Cprofile-use=$DATA/merged.profdata" CARGO_TARGET_DIR=$OUT/pgo $CAP timeout 900 \
    cargo rustc --release -p rn-cli "${FEAT[@]}" --bin rnlua -- -C relocation-model=static "${LINK[@]}" </dev/null
fi
echo "PGO binary: $OUT/pgo/release/rnlua"
