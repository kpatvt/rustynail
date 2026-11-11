#!/bin/bash
# One-command PGO release build of the shipped configuration (rnlua with the JIT + trace compiler, PIE).
# A thin wrapper around scripts/pgo.sh (instrumented build -> training run over scripts/train.sh -> llvm-profdata
# merge -> optimised build); see the P-I, T2 and BUNDLE-A sections of docs/JIT_TRACE_REF.md for the numbers.
#
#   scripts/pgo-build.sh [out-dir]          -> <out-dir>/pgo/release/rnlua   (default out-dir: target/pgo-jit)
#   LUA_VERSION=5.4 scripts/pgo-build.sh    -> the 5.4 build
#   PGO_REPEAT=2 scripts/pgo-build.sh       -> train twice (a bit more stable profile)
#   PGO_HOLDOUT=1 scripts/pgo-build.sh      -> train without the benchmark sets (they stay a hold-out)
#
# Toolchain requirement: `llvm-profdata` matching rustc's LLVM (the rustup component llvm-tools-preview, which
# rust-toolchain.toml lists and scripts/pgo.sh installs on demand; or point LLVM_PROFDATA at one). The plain
# `cargo build --release` does not depend on any of this. No profile data is committed: the merged profile (about
# 2 MB) is tied to the exact rustc and source, so it is regenerated here (roughly 10 minutes: instrumented build,
# training, final build). Output and profile live under <out-dir>, outside git.
#
# REGENERATE after any change to hot paths (interpreter, tier-1 templates, trace compiler, GC, table or string
# code): a stale profile still builds (LLVM ignores profile data of functions whose CFG changed) but silently loses
# the gain there. The script always starts from a fresh profile, so running it again IS the regeneration step.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
OUT=${1:-target/pgo-jit}
PGO_FEATURES=${PGO_FEATURES:-jit} PGO_OUT="$OUT" scripts/pgo.sh
echo "PGO binary: $OUT/pgo/release/rnlua  (profile: $OUT/pgo-data/merged.profdata, not committed)"
