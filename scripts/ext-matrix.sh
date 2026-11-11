#!/bin/bash
# scripts/ext-matrix.sh [ext-diff args...]: runs scripts/ext-diff.sh under the JIT / GC mode matrix
# (default, JIT off, JIT always, traces always, incremental GC, GC stress 1 and 2).
# Heavy tests (name *_heavy*) are skipped under GC stress. Exit status: non-zero if any mode failed.
here=$(cd "$(dirname "$0")" && pwd)
rc=0
for m in "X=default" "RUSTYNAIL_JIT=off" "RUSTYNAIL_JIT=always" "RUSTYNAIL_JIT_TRACE=always" \
         "RUSTYNAIL_GC_MODE=inc" "RUSTYNAIL_GC_STRESS=2" "RUSTYNAIL_GC_STRESS=1"; do
  echo "== $m"
  env "$m" "$here/ext-diff.sh" "$@" || rc=1
done
exit $rc
