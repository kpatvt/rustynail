#!/bin/bash
# Builds the release artifacts and copies them to dist/ (or --out DIR):
#
#   dist/rnlua            the interpreter (`lua`-compatible), JIT on by default, PGO build, stripped
#   dist/rnluac           the bytecode compiler (`luac`-compatible), stripped
#   dist/LICENSE  dist/LICENSE-LUA  dist/NOTICE   (the licence texts are also embedded: `rnlua --license`)
#   dist/SHA256SUMS
#
# usage: scripts/release-build.sh [--no-pgo] [--lua54] [--out DIR]
#   (default)  rnlua from the profile-guided build (scripts/pgo-build.sh: instrumented build, training run, optimised
#              build; about 10 minutes, not bit-reproducible); rnluac comes from the plain build
#   --no-pgo   plain `cargo build --release -p rn-cli` (PIE; the JIT is a default feature)
#   --pgo      accepted for compatibility (PGO is the default)
#   --lua54    the Lua 5.4 build (`--cfg rustynail_lua54`, target-54/)
#   --out DIR  output directory (default dist; removed first)
# Both builds remap build-machine paths (scripts/release-env.sh: source tree -> /rustynail, cargo home -> /cargo, ...),
# the shipped files are stripped (executables --strip-all) and checked for leftover home/user/host names.
# `rnvmrun` (crates/rustynail) is a development/test tool and is not part of the release.
# Every cargo step runs memory-capped (CLAUDE.md). For the full matrix (5.5 + 5.4, several variants, the C API library,
# BUILD-INFO) see scripts/release.sh.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
PGO=1; V54=0; OUT=dist
while [ $# -gt 0 ]; do
  case "$1" in
    --pgo) PGO=1; shift ;;
    --no-pgo) PGO=0; shift ;;
    --lua54) V54=1; shift ;;
    --out) OUT=$2; shift 2 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
$CAP true </dev/null 2>/dev/null || CAP=""   # no user systemd (e.g. CI runners): uncapped
. scripts/release-env.sh

python3 scripts/gen-notice.py --check   # NOTICE must match the dependency graph

if [ "$V54" = 1 ]; then
  T=target-54; VF="--cfg rustynail_lua54"; export LUA_VERSION=5.4; SERIES=5.4
else
  T=target; VF=""; SERIES=5.5
fi
export CFLAGS="$RN_CFLAGS"
START=$(date +%s)
BINS=(); [ "$PGO" = 1 ] && BINS=(--bin rnluac)   # (the PGO build supplies rnlua)
RUSTFLAGS="$VF $RN_REMAP" CARGO_TARGET_DIR=$T $CAP timeout 1800 cargo build --release -p rn-cli "${BINS[@]}" </dev/null
RNLUA=$T/release/rnlua
if [ "$PGO" = 1 ]; then
  # (pgo.sh sets its own RUSTFLAGS from LUA_VERSION and adds the path remapping itself)
  env -u RUSTFLAGS scripts/pgo-build.sh "$T/pgo-jit"
  RNLUA=$T/pgo-jit/pgo/release/rnlua
fi
echo "build time: $(( $(date +%s) - START )) s (PGO=$PGO)"

rm -rf "$OUT"; mkdir -p "$OUT"
cp "$RNLUA" "$OUT/rnlua"
cp "$T/release/rnluac" "$OUT/rnluac"
cp LICENSE LICENSE-LUA NOTICE "$OUT/"
rn_strip "$OUT/rnlua"; rn_strip "$OUT/rnluac"
rn_privacy_check "$OUT/rnlua" "$OUT/rnluac"

# smoke tests: version lines, licence text, a small program
got=$("$OUT/rnlua" -v </dev/null); got=${got%%$'\n'*}
case "$got" in "Lua $SERIES"*) ;; *) echo "smoke test failed: $got" >&2; exit 1 ;; esac
# (no `| grep -q`: an early-closing reader would make the writer die with EPIPE under pipefail)
v=$("$OUT/rnlua" --version </dev/null);  case "$v" in *"Kiran Pashikanti"*) ;; *) echo "bad --version: $v" >&2; exit 1 ;; esac
v=$("$OUT/rnlua" --license </dev/null);  case "$v" in *"PUC-Rio"*) ;; *) echo "bad --license" >&2; exit 1 ;; esac
v=$("$OUT/rnluac" --license </dev/null); case "$v" in *"Kiran Pashikanti"*) ;; *) echo "bad rnluac --license" >&2; exit 1 ;; esac
echo 'local t = {} for i = 1, 1000 do t[i] = i * i end assert(#t == 1000 and t[10] == 100)' | "$OUT/rnlua" - >/dev/null
(cd "$OUT" && sha256sum rnlua rnluac LICENSE LICENSE-LUA NOTICE > SHA256SUMS)
echo "release artifacts in $OUT:"
"$OUT/rnlua" --version </dev/null
cat "$OUT/SHA256SUMS"
