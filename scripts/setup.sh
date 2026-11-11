#!/usr/bin/env bash
# rustynail toolchain + dependency setup (Arch / Arch Linux).
# Idempotent: safe to re-run. Only the pacman step uses sudo.
#
# NOTE: valgrind SIGILLs on glibc binaries on znver4 (AVX-512 ld.so). Use musl-target
# builds under valgrind (verified); perf stat is the primary instruction-count signal.
# Usage: scripts/setup.sh [--no-cargo-tools] [--no-nightly] [--check]
#   --no-cargo-tools  skip `cargo install` of dev tools (faster)
#   --no-nightly      skip nightly toolchain (Miri, sanitizers, cargo-fuzz won't work)
#   --check           only report what is installed, change nothing
set -euo pipefail

CARGO_TOOLS=1; NIGHTLY=1; CHECK=0
for a in "$@"; do
  case "$a" in
    --no-cargo-tools) CARGO_TOOLS=0 ;;
    --no-nightly) NIGHTLY=0 ;;
    --check) CHECK=1 ;;
    -h|--help) sed -n '2,9p' "$0"; exit 0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done

log() { printf '\n\033[1;34m==> %s\033[0m\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }

PACMAN_PKGS=(
  base-devel git curl
  rustup            # provides rustup/cargo/rustc proxies (do NOT also install `rust`)
  clang llvm lld mold
  make gcc
  python            # xtask helper scripts
  hyperfine         # benchmark timing
  perf              # CPU counters / profiling
  valgrind          # cachegrind / callgrind for instruction-count gates
  lua luajit        # informational baselines
  gdb lldb          # debugging
)
CARGO_TOOL_LIST=(
  cargo-nextest cargo-fuzz cargo-llvm-cov cargo-pgo
  cargo-deny flamegraph
)

report() {
  log "Status"
  for t in rustup cargo rustc gdb lldb clang gcc make hyperfine perf valgrind python lua luajit; do
    if have "$t"; then printf '  ok       %-12s %s\n' "$t" "$(command -v "$t")"
    else printf '  MISSING  %s\n' "$t"; fi
  done
  if have rustup; then
    rustup target list --installed | grep -q x86_64-unknown-linux-musl && echo '  ok       musl target' || echo '  MISSING  musl target'
    rustup toolchain list | grep -q nightly && echo '  ok       nightly toolchain' || echo '  MISSING  nightly toolchain'
    cargo +nightly miri --version >/dev/null 2>&1 && echo '  ok       miri' || echo '  MISSING  miri'
  fi
  if have cargo; then
    for t in nextest fuzz llvm-cov pgo deny flamegraph; do
      if cargo "$t" --version >/dev/null 2>&1 || cargo "$t" --help >/dev/null 2>&1; then
        printf '  ok       cargo-%s\n' "$t"; else printf '  MISSING  cargo-%s\n' "$t"; fi
    done
  fi
}

if [ "$CHECK" = 1 ]; then report; exit 0; fi

if ! have pacman; then
  echo "pacman not found: this script targets Arch Linux. Install the packages in PACMAN_PKGS manually." >&2
  exit 1
fi

log "Installing system packages (sudo)"
# -Syu avoids 404s from a stale sync db; --needed skips already-installed packages.
sudo pacman -Syu --needed --noconfirm "${PACMAN_PKGS[@]}"

log "Configuring Rust toolchains"
rustup default stable
rustup component add clippy rustfmt llvm-tools-preview rust-src
# musl target: static binaries run under valgrind (glibc on znver4 crashes it with SIGILL).
rustup target add x86_64-unknown-linux-musl
if [ "$NIGHTLY" = 1 ]; then
  rustup toolchain install nightly --profile minimal \
    --component miri,rust-src,llvm-tools-preview,clippy,rustfmt
  cargo +nightly miri setup
fi

if [ "$CARGO_TOOLS" = 1 ]; then
  log "Installing cargo tools (this compiles from source; can take a while)"
  for t in "${CARGO_TOOL_LIST[@]}"; do
    cargo install --locked "$t" || echo "WARN: failed to install $t (continuing)" >&2
  done
fi

if [ -r /proc/sys/kernel/perf_event_paranoid ] && \
   [ "$(cat /proc/sys/kernel/perf_event_paranoid)" -gt 1 ]; then
  log "Note: perf_event_paranoid > 1; for 'perf stat' as a normal user run:"
  echo "  sudo sysctl kernel.perf_event_paranoid=1"
fi

report
log "Done. Open a new shell (or 'source ~/.cargo/env') so cargo is on PATH."
