#!/bin/bash
# Sourced by scripts/gate-*.sh (and usable by hand: `. scripts/fast-build-env.sh`): optional faster DEV/TEST builds.
# Everything is conditional on the tool existing, a machine without them builds exactly as before. NEVER sourced by the release scripts
# (scripts/release-build.sh, pgo*.sh): the shipped binary is linked with the default linker so it stays reproducible.
#
#   mold      (pacman -S mold | apt install mold), OPT-IN with GATE_FASTLINK=1: linker of the test/dev builds. Off by default: rustc >= 1.90
#             links x86_64 Linux with rust-lld already and measured here mold gains nothing (docs/GATES.md "Builds")
#   sccache   (pacman -S sccache | cargo install sccache --locked): RUSTC_WRAPPER for the third-party crates; workspace
#             crates are keyed by absolute path, so a worktree gets hits only for dependencies (see docs/GATES.md for numbers)
#
# GATE_SCCACHE=0 turns the wrapper part off.
_here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ "${GATE_FASTLINK:-0}" = 1 ] && command -v mold >/dev/null 2>&1 && [ "$(uname -m)" = x86_64 ] && [ "$(uname -s)" = Linux ]; then
  export CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER="$_here/ld-fast.sh"
fi
if [ "${GATE_SCCACHE:-1}" != 0 ] && command -v sccache >/dev/null 2>&1 && [ -z "${RUSTC_WRAPPER:-}" ]; then
  export RUSTC_WRAPPER=sccache
fi
unset _here
