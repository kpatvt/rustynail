#!/bin/bash
# BOLT-optimized rnlua (optional, post-link): binary <target>/bolt/rnlua.bolt
# (<target>/pgo-bolt/rnlua.bolt with BOLT_PGO=1; <target> = target for 5.5, target-54 for LUA_VERSION=5.4).
#   1. builds rnlua like scripts/build-fast.sh (absolute dispatch table) plus --emit-relocs, which BOLT
#      needs to rewrite a binary (<target>/bolt-build/release/rnlua); with BOLT_PGO=1 the base is instead
#      the PGO build of scripts/pgo.sh linked with --emit-relocs (PGO + BOLT);
#   2. samples last-branch records (`perf record -j any,u`) while the base binary runs the training mix
#      (scripts/train.sh: bench/train, bench/micro, tests/vm, part of the official suite; deliberately not
#      bench/programs). The sampling period is BOLT_PERIOD cycles (default 200003, ~10x perf's default
#      4 kHz rate at 4-5 GHz) and the mix runs BOLT_REPEAT times (default 3): about 30x the samples of the
#      first experiment, whose profile was too thin (BOLT asked for ~12x more);
#   3. perf2bolt converts the samples, llvm-bolt reorders blocks (ext-tsp), splits cold code and reorders
#      functions (cdsort).
# The point is code layout: moving the same interpreter code changes cycles by up to +-15% at identical
# instruction counts (docs/PERF.md), so a profile-driven layout is worth trying on top of build-fast/PGO.
# Env: BOLT_BASE=<rnlua binary linked with --emit-relocs> bolts that binary instead of building one.
#      BOLT_PGO=1 builds the PGO base WITHOUT the JIT unless PGO_FEATURES=jit is set (BUNDLE-P: for the shipped JIT
#      configuration use `PGO_FEATURES=jit BOLT_PGO=1 scripts/bolt.sh`, or relink the PGO profile with
#      `-C link-arg=-Wl,--emit-relocs` and pass it as BOLT_BASE). BUNDLE-P measured BOLT on top of PGO+JIT (173 programs,
#      BOLT_PERIOD=100003 BOLT_REPEAT=6 and the default 200003 / 3): 0.999 / 1.000 of the PGO cycles, no set below
#      0.98: NOT worth shipping. BOLT_PERIOD below ~100000 is throttled (perf_event_max_sample_rate).
# Needs llvm-bolt, perf2bolt and perf with branch sampling (scripts/setup-bolt.sh checks).
# usage: [LUA_VERSION=5.4] [BOLT_PGO=1] scripts/bolt.sh
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
# (path remapping: both base builds go through scripts/pgo.sh / scripts/build-fast.sh, which source scripts/release-env.sh;
#  the BOLT output keeps its symbols: strip it before shipping, scripts/release.sh does)
for t in llvm-bolt perf2bolt perf; do command -v $t >/dev/null || { echo "$t not found (scripts/setup-bolt.sh)"; exit 1; }; done
case "${LUA_VERSION:-5.5}" in
  5.5) T=target ;;
  5.4) T=target-54 ;;
  *) echo "LUA_VERSION must be 5.5 or 5.4"; exit 2 ;;
esac
CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
$CAP true </dev/null 2>/dev/null || CAP=""   # no user systemd (e.g. CI runners): uncapped

if [ -n "${BOLT_BASE:-}" ]; then
  BASE=$(realpath "$BOLT_BASE")
  OUT=$T/bolt
elif [ "${BOLT_PGO:-0}" = 1 ]; then
  PGO_EMIT_RELOCS=1 scripts/pgo.sh
  BASE=$(pwd)/$T/pgo/release/rnlua
  OUT=$T/pgo-bolt
else
  CARGO_TARGET_DIR=$T/bolt-build scripts/build-fast.sh -C link-arg=-Wl,--emit-relocs
  BASE=$(pwd)/$T/bolt-build/release/rnlua
  OUT=$T/bolt
fi
mkdir -p "$OUT"
cp "$BASE" "$OUT/rnlua.base"

# one perf session over the whole training mix (children are followed)
$CAP timeout 3600 perf record -q -e cycles:u -c "${BOLT_PERIOD:-200003}" -j any,u -o "$OUT/perf.data" -- \
  scripts/train.sh "$OUT/rnlua.base" "${BOLT_REPEAT:-3}" </dev/null
perf2bolt -p "$OUT/perf.data" -o "$OUT/perf.fdata" "$OUT/rnlua.base"
llvm-bolt "$OUT/rnlua.base" -o "$OUT/rnlua.bolt" -data="$OUT/perf.fdata" \
  -reorder-blocks=ext-tsp -reorder-functions=cdsort -split-functions -split-all-cold \
  -icf=all -dyno-stats
echo "BOLT binary: $OUT/rnlua.bolt"
