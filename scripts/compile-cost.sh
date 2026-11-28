#!/usr/bin/env bash
# Compile-cost harness (docs/JIT_TRACE.md section 80.0): scripts/compile_cost.py under the memory cap and a timeout.
#   scripts/compile-cost.sh phases target/release/rnlua=new /path/base/rnlua=base [--programs apps/raytrace,...] [--cpu 5]
#   scripts/compile-cost.sh cycles A=a B=b [--runs 15] [--cpu 5]
#   scripts/compile-cost.sh whole A=a B=b [--sets awfy,apps] [--cpu 5]
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
exec systemd-run --user --scope -q -p MemoryMax="${CC_MEM:-8G}" -p MemorySwapMax=0 \
    timeout "${CC_TIMEOUT:-7200}" python3 "$here/compile_cost.py" "$@" </dev/null
