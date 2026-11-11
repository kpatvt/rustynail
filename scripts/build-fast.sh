#!/bin/bash
# Release build of rnlua with an absolute (non-PIC) dispatch jump table: binary <target>/fast/release/rnlua
# (target/fast/... for Lua 5.5, target-54/fast/... for LUA_VERSION=5.4).
#
# In a PIE every `match` jump table is relative (load offset, add table base, jump: +3 instructions per
# bytecode dispatch). `-C relocation-model=static` gives absolute tables, but applied to the whole build it
# also makes `librn_capi.a` unlinkable into PIE programs. With fat LTO all code is generated in the final
# crate's session, so passing the flag to the `rnlua` binary crate alone (`cargo rustc ... -- <flags>`)
# turns the whole interpreter into static-relocation code while every library crate (and the C API
# staticlib, built separately) keeps the default model. The executable is non-PIE but still exports the
# C API (`-rdynamic`, see crates/rn-cli/build.rs) for C modules.
# usage: [LUA_VERSION=5.4] [CARGO_TARGET_DIR=dir] scripts/build-fast.sh [extra rustc flags...]
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
. scripts/release-env.sh   # RN_REMAP / RN_CFLAGS: no build-machine paths in the binary
case "${LUA_VERSION:-5.5}" in
  5.5) T=target ;;
  5.4) T=target-54; export RUSTFLAGS="--cfg rustynail_lua54 ${RUSTFLAGS:-}" ;;
  *) echo "LUA_VERSION must be 5.5 or 5.4"; exit 2 ;;
esac
CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
$CAP true </dev/null 2>/dev/null || CAP=""   # no user systemd (e.g. CI runners): uncapped
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$T/fast}"
rn_remap_target "$(realpath -m "$CARGO_TARGET_DIR")"; export CFLAGS="$RN_CFLAGS"
export RUSTFLAGS="$RN_REMAP ${RUSTFLAGS:-}"
$CAP timeout 900 cargo rustc --release -p rn-cli --bin rnlua -- -C relocation-model=static "$@" </dev/null
echo "fast binary: $CARGO_TARGET_DIR/release/rnlua"
