#!/bin/bash
# The JIT promise (docs/JIT_PLAN.md, JIT_HOOK_COST.md): a build WITHOUT the `jit` feature has the same machine
# code as before the JIT hooks existed. This builds rnlua (release, default features) at <base-ref> and at
# the working tree's HEAD in scratch worktrees and compares the `.text` sections.
#
#   scripts/jit-text-identity.sh [base-ref]        (default: HEAD^1 on a merge commit, else origin/main)
#   LUA_VERSION=5.4 scripts/jit-text-identity.sh ...
#   scripts/jit-text-identity.sh --fast-copy       (J5, docs/JIT_TRACE.md TR23.1: no base ref)
#     Builds rnlua (release) of the working tree with `--features jit_j4` (J4 only; `jit` includes traces since F9) and with `--features jit_trace` (jit + the
#     recorder hook) and compares the *relocation-normalised disassembly* of the hot interpreter copy
#     `rn_core::vm_unsafe::run_mode::<false>` (scripts/fastcopy-norm.py: branch targets relative to the function,
#     rip-relative displacements / absolute addresses / symbol annotations masked). Exit 0: identical instruction
#     sequence. The hooked copy `run_mode::<true>` legitimately differs and is not compared.
# Exit 0: identical. Exit 1: different (expected only for a change that intentionally alters the interpreter
# or rn-core/std/compiler code; review it). Needs `objcopy`; builds are niced and capped.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
FAST=0
if [ "${1:-}" = "--fast-copy" ]; then FAST=1; shift; fi
BASE=${1:-}
if [ -z "$BASE" ]; then
  if git rev-parse -q --verify HEAD^2 >/dev/null; then BASE=HEAD^1; else BASE=origin/main; fi
fi
LV=${LUA_VERSION:-5.5}
RF=""; [ "$LV" = 5.4 ] && RF="--cfg rustynail_lua54"
if [ "$FAST" = 1 ]; then
  W=$(mktemp -d)
  trap 'rm -rf "$W"' EXIT
  CAP=(systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0)
  "${CAP[@]}" true </dev/null 2>/dev/null || CAP=()
  for f in jit_j4 jit_trace; do
    env RUSTFLAGS="$RF" CARGO_TARGET_DIR="$W/target-$f" nice "${CAP[@]}" timeout 3000 \
      cargo build --release -p rn-cli --no-default-features --features "$f" --bin rnlua </dev/null >/dev/null 2>&1
    python3 scripts/fastcopy-norm.py "$W/target-$f/release/rnlua" 'rn_core::vm_unsafe::run_mode::<false>' >"$W/$f.dis"
  done
  if cmp -s "$W/jit_j4.dis" "$W/jit_trace.dis"; then
    echo "FAST COPY IDENTICAL ($LV): jit_j4 vs jit_trace, $(wc -l <"$W/jit_j4.dis") instructions"
  else
    echo "FAST COPY DIFFERS ($LV): jit_j4 vs jit_trace ($(wc -l <"$W/jit_j4.dis") vs $(wc -l <"$W/jit_trace.dis") instructions)"
    diff "$W/jit_j4.dis" "$W/jit_trace.dis" | head -40; exit 1
  fi
  exit 0
fi
W=$(mktemp -d)
trap 'git worktree remove --force "$W/base" 2>/dev/null; git worktree remove --force "$W/head" 2>/dev/null; rm -rf "$W"' EXIT
CAP=(systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0)
"${CAP[@]}" true </dev/null 2>/dev/null || CAP=()
build() { # dir
  (cd "$1" && env RUSTFLAGS="$RF" CARGO_TARGET_DIR="$W/target-$(basename "$1")" nice "${CAP[@]}" timeout 3000 \
     cargo build --release -p rn-cli --no-default-features --bin rnlua </dev/null >/dev/null 2>&1)
  objcopy --only-section=.text "$W/target-$(basename "$1")/release/rnlua" "$W/$(basename "$1").text"
}
git worktree add -q --detach "$W/base" "$BASE"
git worktree add -q --detach "$W/head" HEAD
build "$W/base"
build "$W/head"
if cmp -s "$W/base.text" "$W/head.text"; then
  echo "TEXT IDENTICAL ($LV): $BASE vs HEAD, $(stat -c %s "$W/head.text") bytes"
else
  echo "TEXT DIFFERS ($LV): $BASE vs HEAD ($(stat -c %s "$W/base.text") vs $(stat -c %s "$W/head.text") bytes)"; exit 1
fi
