#!/usr/bin/env bash
# Install LLVM BOLT (post-link optimizer) and check that it works with perf on this machine.
# Needs sudo (password prompt). Arch Linux: package `llvm-bolt` (must match the installed `llvm`).
set -euo pipefail

if ! command -v llvm-bolt >/dev/null 2>&1; then
    echo "== installing llvm-bolt"
    sudo pacman -S --needed llvm-bolt
fi

echo "== versions"
llvm-bolt --version | head -3
command -v perf2bolt >/dev/null || { echo "perf2bolt missing from PATH (package layout changed?)"; exit 1; }
echo "llvm:  $(llvm-config --version 2>/dev/null || echo unknown)"

echo "== perf branch sampling (LBR / branch stack)"
if [ "$(cat /proc/sys/kernel/perf_event_paranoid)" -gt 2 ]; then
    echo "perf_event_paranoid > 2; lower it: sudo sysctl kernel.perf_event_paranoid=2"
    exit 1
fi
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
if perf record -q -b -o "$tmp/perf.data" -- true >/dev/null 2>&1; then
    echo "perf record -b works"
else
    echo "perf record -b failed: BOLT needs branch-stack sampling (try: perf record -e cycles:u -j any,u)"
    exit 1
fi

echo "== OK: llvm-bolt, perf2bolt and perf -b are available"
