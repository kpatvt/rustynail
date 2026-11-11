#!/bin/bash
# Post-M full benchmark driver: stage 1 single-core calibration (12 programs, CPU 4 only), stage 2 four-way default pass,
# stage 3 four-way warm0 pass. Sequential; writes its PID to bench-mfull-all.pid and exits when the last stage is done
# (no respawning). Progress in docs/bench-mfull-*.err; marker file docs/bench-mfull-ALLDONE at the end.
set -u
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
echo $$ > "$here/bench-mfull-all.pid"
CAL=awfy/cd,awfy/deltablue,awfy/json,awfy/towers,awfy/richards,apps/raytrace,apps/expr_lang,programs/fib,jit-trace/oop_override,lib/find_plain,lib/gsub_str,real/lua_interp
echo "stage 1 $(date +%T)" >&2
CPUS=4 MODE=default ONLY=$CAL OUTPFX="$here/docs/bench-mfull-single" scripts/bench-mfull.sh 2> docs/bench-mfull-single.err
echo "stage 2 $(date +%T)" >&2
CPUS=4,5,6,7 MODE=default OUTPFX="$here/docs/bench-mfull-default" scripts/bench-mfull.sh 2> docs/bench-mfull-default.err
echo "stage 3 $(date +%T)" >&2
CPUS=4,5,6,7 MODE=warm0 OUTPFX="$here/docs/bench-mfull-warm0" scripts/bench-mfull.sh 2> docs/bench-mfull-warm0.err
echo "done $(date +%T)" > "$here/docs/bench-mfull-ALLDONE"
rm -f "$here/bench-mfull-all.pid" "$here/bench-mfull.pid"
