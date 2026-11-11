#!/bin/bash
# Tier-1 machine code must stay byte-identical across IR/backend changes (docs/JIT_TIER15.md 4.2, task T1).
# Builds rnlua (release, --features jit) at <base-ref> and at the working tree's HEAD in scratch worktrees,
# runs every bench/programs/*.lua with RUSTYNAIL_JIT=always RUSTYNAIL_JIT_DUMP=dir (a small argument keeps the
# runs short; the code is compiled at the first call of each function), and compares the dumped machine code.
# The dump file names and the embedded absolute heap addresses (`movabs` immediates) differ from run to run,
# so each proto is disassembled, `movabs` immediates are masked, and the sorted multiset of digests is compared.
#
#   scripts/jit-dump-identity.sh [base-ref]        (default: HEAD^1)
#   LUA_VERSION=5.4 scripts/jit-dump-identity.sh ...
#   JITDUMP_FEATURES_HEAD=jit,trace (J5 P9): cargo features of the head build (base: JITDUMP_FEATURES_BASE, default jit);
#   the environment (e.g. RUSTYNAIL_JIT_TRACE=off RUSTYNAIL_JIT_T15=on) is inherited by the dumped runs.
# Exit 0: identical. Needs objdump.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
# F9: traces are on by default in `jit` builds; this script compares tier-1 code (the J4 behaviour), so pin it unless the caller chose
export RUSTYNAIL_JIT_TRACE=${RUSTYNAIL_JIT_TRACE:-off}
BASE=${1:-HEAD^1}
LV=${LUA_VERSION:-5.5}
RF=""; [ "$LV" = 5.4 ] && RF="--cfg rustynail_lua54"
W=$(mktemp -d "${TMPDIR:-/tmp}/jitdump.XXXXXX")
trap 'git worktree remove --force "$W/base" 2>/dev/null; git worktree remove --force "$W/head" 2>/dev/null; rm -rf "$W"' EXIT
CAP=(systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0)
"${CAP[@]}" true </dev/null 2>/dev/null || CAP=()
ROOT=$PWD
build() { # dir features
  (cd "$1" && env RUSTFLAGS="$RF" CARGO_TARGET_DIR="$W/target-$(basename "$1")" nice "${CAP[@]}" timeout 3000 \
     cargo build --release -p rn-cli --no-default-features --features "$2" --bin rnlua </dev/null >/dev/null 2>&1)
}
dump() { # tag
  local bin="$W/target-$1/release/rnlua" out="$W/out-$1"
  mkdir -p "$out"
  for p in "$ROOT"/bench/programs/*.lua; do
    n=$(basename "$p" .lua)
    d="$out/$n"; mkdir -p "$d"
    (cd "$ROOT" && RUSTYNAIL_JIT=always RUSTYNAIL_JIT_DUMP="$d" RUSTYNAIL_SEED=0 \
       "${CAP[@]}" timeout 120 "$bin" "$p" 2 </dev/null >/dev/null 2>&1) || true
    for f in "$d"/*.bin; do
      [ -e "$f" ] || continue
      objdump -D --no-show-raw-insn -b binary -mi386:x86-64 -M intel "$f" | tail -n +8 \
        | sed -E 's/(movabs +[a-z0-9]+,)0x[0-9a-f]+/\1IMM/' | md5sum | cut -d' ' -f1
    done | sort > "$out/$n.digests"
    rm -rf "$d"
  done
}
git worktree add -q --detach "$W/base" "$BASE"
git worktree add -q --detach "$W/head" HEAD
build "$W/base" "${JITDUMP_FEATURES_BASE:-jit}"
build "$W/head" "${JITDUMP_FEATURES_HEAD:-jit}"
dump base
dump head
rc=0
total=0
for f in "$W"/out-base/*.digests; do
  n=$(basename "$f")
  total=$((total + $(wc -l < "$f")))
  if ! cmp -s "$f" "$W/out-head/$n"; then echo "DIFFERENT: $n"; rc=1; fi
done
echo "compared $(ls "$W"/out-base/*.digests | wc -l) programs, $total protos"
[ $rc = 0 ] && echo "IDENTICAL"
exit $rc
