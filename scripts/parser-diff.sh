#!/usr/bin/env bash
# Parser/compiler differential vs the reference front end (crates/rn-std/tests/parser_diff.rs).
#
#   scripts/parser-diff.sh [--lua-version 5.4|5.5] [--cases N] [--seed S] [--out DIR] [--min] [--jobs J]
#
# Defaults: 5.5, 200000 generated cases (+ corpus files, edge cases and every-byte truncations of the
# small corpus files), seed 1, mismatches saved to target/parser-diff-<ver>/ (with --min, the first
# 20 of each category are also delta-minimized to *.min.lua). Runs under a memory cap and a timeout.
set -euo pipefail
cd "$(dirname "$0")/.."
ver=5.5 cases=200000 seed=1 out="" min="" jobs=""
while [ $# -gt 0 ]; do
  case "$1" in
    --lua-version) ver=$2; shift 2 ;;
    --cases) cases=$2; shift 2 ;;
    --seed) seed=$2; shift 2 ;;
    --out) out=$2; shift 2 ;;
    --min) min=1; shift ;;
    --jobs) jobs=$2; shift 2 ;;
    *) echo "unknown argument $1" >&2; exit 2 ;;
  esac
done
tag=${ver/./}
[ -n "$out" ] || out="$PWD/target/parser-diff-$tag"
rm -rf "$out"
mkdir -p "$out"
flags=() targetdir=()
if [ "$ver" = 5.4 ]; then
  export RUSTFLAGS="${RUSTFLAGS:-} --cfg rustynail_lua54"
  targetdir=(--target-dir target-54)
fi
export RUSTYNAIL_PARSE_DIFF_CASES=$cases RUSTYNAIL_PARSE_DIFF_SEED=$seed RUSTYNAIL_PARSE_DIFF_OUT=$out
[ -n "$min" ] && export RUSTYNAIL_PARSE_DIFF_MIN=1
[ -n "$jobs" ] && export RUSTYNAIL_PARSE_DIFF_JOBS=$jobs
cap=(systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0)
command -v systemd-run >/dev/null || cap=()
"${cap[@]}" timeout 7200 cargo test -p rn-std --release "${targetdir[@]}" "${flags[@]}" \
  --test parser_diff parser_matches_reference -- --nocapture </dev/null 2>&1 | tee "$out/log.txt"
