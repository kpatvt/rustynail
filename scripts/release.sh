#!/bin/bash
# Build release artifacts into dist/: for each Lua version and build variant the interpreter, plus (in the
# `default` variant) the bytecode compiler and the C API static library with its headers; LICENSE files,
# a BUILD-INFO file and SHA256SUMS.
#
# Every shipped executable/library is stripped (scripts/release-env.sh) and checked for build-machine strings;
# BUILD-INFO.txt records only commit, date, rustc, target triple and `uname -sm`.
# usage: scripts/release.sh [--versions 5.5,5.4] [--variants pgo,default,fast,bolt,pgo-bolt] [--out dist]
#   default   cargo build --release (PIE; the reproducible build, see docs/REPRODUCIBLE.md; JIT on since BUNDLE W)
#   fast      scripts/build-fast.sh (absolute dispatch jump table, non-PIE; also reproducible)
#   pgo       scripts/pgo.sh with PGO_FEATURES=jit (JIT + trace compiler, profile-guided, trained on scripts/train.sh;
#             not bit-reproducible). THE DEFAULT RELEASE BUILD (BUNDLE-P: 0.94 of the plain build's cycles).
#   bolt      scripts/bolt.sh (fast build + BOLT layout; needs llvm-bolt, perf with LBR)
#   pgo-bolt  BOLT_PGO=1 scripts/bolt.sh (PGO build + BOLT layout)
# Default: --versions 5.5,5.4 --variants pgo,default (the `default` variant is the unprofiled build and stays the
# reproducible reference build; the C API library and luac come from it).
#
# Layout:
#   dist/lua-5.5/<variant>/rnlua        (+ rnluac in default)
#   dist/lua-5.5/capi/librn_capi.a, dist/lua-5.5/capi/include/{lua.h,luaconf.h,lualib.h,lauxlib.h,lua.hpp}
#   dist/lua-5.4/...
#   dist/LICENSE, dist/LICENSE-LUA, dist/NOTICE, dist/BUILD-INFO.txt, dist/SHA256SUMS
#
# Reproducibility: the default and fast variants are built with SOURCE_DATE_EPOCH (commit time) and path
# remapping (--remap-path-prefix for rustc, -ffile-prefix-map for the C shim), so two builds of the same
# commit with the same toolchain are bit-identical, wherever the checkout and target directory are.
# Every cargo/training/perf step runs memory-capped (CLAUDE.md).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
export PATH="$HOME/.cargo/bin:$PATH"
VERSIONS=5.5,5.4
VARIANTS=pgo,default
OUT=dist
while [ $# -gt 0 ]; do
  case "$1" in
    --versions) VERSIONS=$2; shift 2 ;;
    --variants) VARIANTS=$2; shift 2 ;;
    --out) OUT=$2; shift 2 ;;
    -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1"; exit 2 ;;
  esac
done
CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
$CAP true </dev/null 2>/dev/null || CAP=""   # no user systemd (e.g. CI runners): uncapped

rm -rf "$OUT"; mkdir -p "$OUT"
OUT=$(realpath "$OUT")
REV=$(git rev-parse HEAD 2>/dev/null || echo unknown)
DIRTY=$(git status --porcelain --untracked-files=no 2>/dev/null | grep -q . && echo "+dirty" || true)
export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct 2>/dev/null || date +%s)}
# path remapping (checkout, cargo home, rustup home, toolchain, home dir): scripts/release-env.sh
. scripts/release-env.sh
REMAP="$RN_REMAP"
TARGET=$(rustc -vV | sed -n 's/^host: //p')

info() { echo "$*" >> "$OUT/BUILD-INFO.txt"; }
info "rustynail release build"
info "commit:            $REV$DIRTY"
info "SOURCE_DATE_EPOCH: $SOURCE_DATE_EPOCH ($(date -u -d "@$SOURCE_DATE_EPOCH" +%FT%TZ))"
info "target:            $TARGET"
info "platform:          $(uname -sm)"
info "rustc:             $(rustc -V)"
info "profile:           release (opt-level 3, lto fat, codegen-units 1, debug 1, panic unwind)"
info "versions:          $VERSIONS"
info "variants:          $VARIANTS"
info ""

for V in ${VERSIONS//,/ }; do
  case "$V" in
    5.5) T=target; VF="" ;;
    5.4) T=target-54; VF="--cfg rustynail_lua54" ;;
    *) echo "unknown Lua version $V"; exit 2 ;;
  esac
  D=$OUT/lua-$V
  for VAR in ${VARIANTS//,/ }; do
    mkdir -p "$D/$VAR"
    case "$VAR" in
      default)
        TD=$ROOT/$T/dist
        RUSTFLAGS="$VF $REMAP --remap-path-prefix=$TD=/target" CFLAGS="$RN_CFLAGS -ffile-prefix-map=$TD=/target" \
          CARGO_TARGET_DIR=$TD $CAP timeout 1200 cargo build --release -p rn-cli -p rn-capi </dev/null
        cp "$TD/release/rnlua" "$TD/release/rnluac" "$D/$VAR/"
        mkdir -p "$D/capi/include"
        cp "$TD/release/librn_capi.a" "$D/capi/"
        rn_strip "$D/capi/librn_capi.a"; rn_privacy_check "$D/capi/librn_capi.a"
        for h in lua.h luaconf.h lualib.h lauxlib.h lua.hpp; do
          cp "vendor/lua-$V/src/$h" "$D/capi/include/"
        done
        FLAGS="RUSTFLAGS=\"$VF <remap>\""
        ;;
      fast)
        TD=$ROOT/$T/dist-fast
        RUSTFLAGS="$REMAP --remap-path-prefix=$TD=/target" CFLAGS="$RN_CFLAGS -ffile-prefix-map=$TD=/target" \
          CARGO_TARGET_DIR=$TD LUA_VERSION=$V scripts/build-fast.sh
        cp "$TD/release/rnlua" "$D/$VAR/"
        FLAGS="RUSTFLAGS=\"$VF <remap>\" + rnlua: -C relocation-model=static"
        ;;
      pgo)
        LUA_VERSION=$V PGO_FEATURES=${PGO_FEATURES:-jit} scripts/pgo.sh
        cp "$T/pgo/release/rnlua" "$D/$VAR/"
        FLAGS="scripts/pgo.sh PGO_FEATURES=${PGO_FEATURES:-jit} (-Cprofile-use, PIE)"
        ;;
      bolt)
        LUA_VERSION=$V scripts/bolt.sh
        cp "$T/bolt/rnlua.bolt" "$D/$VAR/rnlua"
        FLAGS="scripts/bolt.sh ($(llvm-bolt --version 2>/dev/null | grep -m1 -i version | xargs))"
        ;;
      pgo-bolt)
        LUA_VERSION=$V BOLT_PGO=1 scripts/bolt.sh
        cp "$T/pgo-bolt/rnlua.bolt" "$D/$VAR/rnlua"
        FLAGS="BOLT_PGO=1 scripts/bolt.sh"
        ;;
      *) echo "unknown variant $VAR"; exit 2 ;;
    esac
    # ship stripped files only (the unstripped build outputs stay in the target dirs), and check for build-machine names
    for f in "$D/$VAR"/rnlua "$D/$VAR"/rnluac; do [ -f "$f" ] && rn_strip "$f"; done
    rn_privacy_check "$D/$VAR"/rnlua
    # smoke test: the binary runs and reports the right version
    got=$("$D/$VAR/rnlua" -v </dev/null 2>&1 | head -1)
    case "$got" in "Lua $V"*) ;; *) echo "smoke test failed for $V/$VAR: $got"; exit 1 ;; esac
    echo 'local t = {} for i = 1, 1000 do t[i] = i * i end assert(#t == 1000 and t[10] == 100)' |
      "$D/$VAR/rnlua" - >/dev/null
    info "lua-$V/$VAR: $FLAGS"
  done
done
cp LICENSE LICENSE-LUA NOTICE "$OUT/"
(cd "$OUT" && find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum > SHA256SUMS)
echo "artifacts in $OUT:"
cat "$OUT/SHA256SUMS"
