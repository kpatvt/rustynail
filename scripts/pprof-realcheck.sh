#!/bin/bash
# Output-equality check of bench/real programs: ref vs ours (JIT trace on / off). Usage: pprof-realcheck.sh <ours-rn-cli>
set -u
here="$(cd "$(dirname "$0")/.." && pwd)"
ours="${1:?rnlua}"
ref="${REF:-$here/ref/55/build-seed0/lua}"
for f in "$here"/bench/real/*.lua; do
  a=$(timeout 120 "$ref" "$f" </dev/null 2>&1 | md5sum)
  b=$(timeout 120 "$ours" "$f" </dev/null 2>&1 | md5sum)
  c=$(RUSTYNAIL_JIT_TRACE=off timeout 120 "$ours" "$f" </dev/null 2>&1 | md5sum)
  [ "$a" = "$b" ] && [ "$a" = "$c" ] && echo "same $(basename "$f") $(timeout 120 "$ref" "$f" </dev/null 2>&1 | head -c 80)" || echo "DIFF $(basename "$f")"
done
