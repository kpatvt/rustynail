#!/bin/bash
# JIT hook cost measurement (docs/JIT_HOOK_COST.md).
#
#   scripts/jit-hook-cost.sh build  <outdir>   build rnlua for every variant into <outdir>
#   scripts/jit-hook-cost.sh text   <outdir> <baseline-binary>   .text identity of OFF vs baseline
#   scripts/jit-hook-cost.sh bench  <outdir> <baseline-binary> <cpu> [runs]   perf-stat comparison
#
# Variants = cargo features of rnlua (see FEAT below). Counter: A per-Proto, B thread tick, C per-CallInfo;
# check placement: c 'frame head, d call+return, e merged into the call-time counter. noloop/noentry are
# ablations. VARIANTS="..." selects a subset. Add FAST=1 to use the `fast`
# build (scripts/build-fast.sh, static relocation model) instead of the plain release build.
# PROGS=a,b limits the programs. Builds run niced on cpus 0-2 under a memory cap; benchmarks should be pinned to a quiet cpu.
set -euo pipefail
cd "$(dirname "$0")/.."
cmd=${1:?build|text|bench}; out=${2:?outdir}
CAP="systemd-run --user --scope -q -p MemoryMax=8G -p MemorySwapMax=0"
declare -A FEAT=(
  [off]="" [full]="jit" [A_c]="rn-core/jit" [A_d]="rn-core/jit,jit_check_d" [B_c]="rn-core/jit,jit_counter_b" [B_d]="rn-core/jit,jit_counter_b,jit_check_d"
  [C_c]="rn-core/jit,jit_counter_c" [A_e]="rn-core/jit,jit_check_e" [B_e]="rn-core/jit,jit_check_e,jit_counter_b" [C_e]="rn-core/jit,jit_check_e,jit_counter_c"
  [G_c]="rn-core/jit,jit_counter_g" [G_e]="rn-core/jit,jit_check_e,jit_counter_g" [noentry_G]="rn-core/jit,jit_no_entry,jit_counter_g"
  [noloop_c]="rn-core/jit,jit_no_loop" [noloop_e]="rn-core/jit,jit_check_e,jit_no_loop"
  [noentry_A]="rn-core/jit,jit_no_entry" [noentry_B]="rn-core/jit,jit_no_entry,jit_counter_b" [noentry_C]="rn-core/jit,jit_no_entry,jit_counter_c"
)
ORDER="${VARIANTS:-off A_c A_d B_c B_d C_c A_e B_e C_e noloop_c noloop_e noentry_A noentry_B noentry_C}"
suffix=""; [ "${FAST:-}" = 1 ] && suffix=".fast"
case $cmd in
build)
  mkdir -p "$out"
  export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$out/target}"
  for v in $ORDER; do
    f=${FEAT[$v]}
    if [ "${FAST:-}" = 1 ]; then
      nice -n 19 taskset -c 0-2 $CAP timeout 1500 cargo rustc --release -p rn-cli --bin rnlua --no-default-features ${f:+--features "$f"} -- -C relocation-model=static </dev/null
    else
      nice -n 19 taskset -c 0-2 $CAP timeout 1500 cargo build --release -p rn-cli --bin rnlua --no-default-features ${f:+--features "$f"} </dev/null
    fi
    cp "$CARGO_TARGET_DIR/release/rnlua" "$out/rnlua.$v$suffix"
  done ;;
text)
  base=${3:?baseline binary}
  objcopy --only-section=.text "$base" "$out/base.text"
  for v in $ORDER; do
    objcopy --only-section=.text "$out/rnlua.$v$suffix" "$out/$v.text"
    if cmp -s "$out/base.text" "$out/$v.text"; then echo "$v: .text IDENTICAL"; else echo "$v: .text differs"; fi
  done ;;
bench)
  base=${3:?baseline binary}; cpu=${4:?cpu}; runs=${5:-9}
  bins="base=$base"
  for v in $ORDER; do bins="$bins $v=$out/rnlua.$v$suffix"; done
  $CAP timeout 5400 python3 - "$cpu" "$runs" "$out/bench$suffix.json" $bins </dev/null <<'PY'
import glob, json, math, os, subprocess, sys
cpu, runs, jpath = sys.argv[1], int(sys.argv[2]), sys.argv[3]
bins = [a.split("=", 1) for a in sys.argv[4:]]
progs = sorted(glob.glob("bench/programs/*.lua"))
only = [x for x in os.environ.get("PROGS", "").split(",") if x]
if only: progs = [p for p in progs if os.path.basename(p)[:-4] in only]
def stat(b, p):
    r = subprocess.run(["perf", "stat", "-x,", "-e", "instructions:u,cycles:u", "taskset", "-c", cpu, b, p],
                       stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL)
    d = {}
    for l in r.stderr.decode().splitlines():
        f = l.split(",")
        if len(f) > 2 and f[0].isdigit():
            d[f[2].split(":")[0]] = int(f[0])
    return d
res = {}
for p in progs:
    n = os.path.basename(p)[:-4]
    res[n] = {}
    for k in range(runs):          # interleave binaries to spread drift
        for name, b in bins:
            d = stat(b, p)
            e = res[n].setdefault(name, {"instructions": [], "cycles": []})
            e["instructions"].append(d["instructions"]); e["cycles"].append(d["cycles"])
    print("ran", n, file=sys.stderr, flush=True)
json.dump(res, open(jpath, "w"))
def gm(x): return math.exp(sum(map(math.log, x)) / len(x))
for metric in ("instructions", "cycles"):
    print(f"\n{metric} (min of {runs}) ratio vs base")
    names = [n for n, _ in bins[1:]]
    print(f"{'benchmark':14}" + "".join(f"{n:>9}" for n in names))
    cols = {n: [] for n in names}
    for pn, r in res.items():
        row = f"{pn:14}"
        for n in names:
            x = min(r[n][metric]) / min(r["base"][metric]); cols[n].append(x); row += f"{x:9.4f}"
        print(row)
    print(f"{'GEOMEAN':14}" + "".join(f"{gm(cols[n]):9.4f}" for n in names))
    print(f"{'WORST':14}" + "".join(f"{max(cols[n]):9.4f}" for n in names))
PY
  ;;
esac
