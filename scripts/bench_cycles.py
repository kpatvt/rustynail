#!/usr/bin/env python3
"""Cycle/instruction benchmark for the JIT work (J0): see docs/JIT_BENCH.md and scripts/bench-cycles.sh.

  scripts/bench-cycles.sh name=cmd [name=cmd ...]      (use the wrapper: it adds memory cap + timeout)

For every program of bench/programs and every binary, RUNS runs of
`perf stat -x, -e cycles:u,instructions:u -- taskset -c CPU <cmd> prog.lua`, interleaved (for each run, for each
program, for each binary), so slow drift of the machine hits all binaries alike. Per (program, binary) the
minimum and the median of both counters are kept. The first binary is the baseline of all ratios.

Options (environment): CPU (default 4), RUNS (default 9), ONLY (comma list of programs), OUT (json path),
LJ_BINS (comma list of binary names that use bench/luajit/<prog>.lua when that variant exists),
EXCLUDE (name:prog+prog,name2:prog ... programs not run for a binary),
PROGDIR (directory of the programs, default bench/programs; relative paths are relative to the repository root, e.g.
PROGDIR=bench/jit-trace for the frozen J5 programs, bench/lib, bench/awfy, bench/apps (H1 sets); the bench/luajit
variants are only used without PROGDIR).
If <prog>.expected exists next to a program, the stdout of every binary is compared with it (exact) and recorded as
"expected_ok" (shown as `X` in the table when it differs).
"""
import glob, json, math, os, statistics, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NUMERIC = ["fannkuch", "loops_arith", "mandelbrot", "matmul", "nbody", "sieve", "spectral_norm"]
NOISE = 0.05


def geomean(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def measure(cmd, cpu):
    """Returns (stdout, {event: count}) of one pinned run, or (None, error)."""
    with tempfile.NamedTemporaryFile("r", suffix=".perf") as f:
        full = ["perf", "stat", "-x,", "-o", f.name, "-e", "cycles:u,instructions:u", "--",
                "taskset", "-c", str(cpu)] + cmd
        p = subprocess.run(full, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL)
        if p.returncode != 0:
            return None, p.stderr.decode(errors="replace")[-200:]
        res = {}
        for line in f.read().splitlines():
            fld = line.split(",")
            if len(fld) > 2 and fld[0].isdigit():
                res[fld[2].split(":")[0]] = int(fld[0])
        return p.stdout, res


def main():
    cpu = int(os.environ.get("CPU", "4"))
    runs = int(os.environ.get("RUNS", "9"))
    only = [x for x in os.environ.get("ONLY", "").split(",") if x]
    out_json = os.environ.get("OUT", "")
    lj_bins = set(x for x in os.environ.get("LJ_BINS", "").split(",") if x)
    progdir = os.environ.get("PROGDIR", "")
    custom_dir = bool(progdir)
    progdir = os.path.join(ROOT, progdir or "bench/programs")  # an absolute PROGDIR wins in os.path.join
    exclude = {}
    for item in os.environ.get("EXCLUDE", "").split(","):
        if ":" in item:
            n, ps = item.split(":", 1)
            exclude.setdefault(n, set()).update(ps.split("+"))
    bins = []
    for b in sys.argv[1:]:
        n, c = b.split("=", 1)
        bins.append((n, c.split()))
    if not bins:
        sys.exit(__doc__)
    names = [n for n, _ in bins]
    progs = sorted(os.path.basename(p)[:-4] for p in glob.glob(os.path.join(progdir, "*.lua")))
    if only:
        progs = [p for p in progs if p in only]

    def expected_for(prog):
        e = os.path.join(progdir, prog + ".expected")
        return open(e, "rb").read() if os.path.exists(e) else None

    def path_for(bn, prog):
        if bn in lj_bins and not custom_dir:
            v = os.path.join(ROOT, "bench/luajit", prog + ".lua")
            if os.path.exists(v):
                return v, True
        return os.path.join(progdir, prog + ".lua"), False

    samples, outputs, notes, variant = {}, {}, {}, {}
    active = {p: [n for n in names if p not in exclude.get(n, ())] for p in progs}
    for p in progs:  # warm-up (also checks that it runs and compares output with the baseline)
        for n, cmd in bins:
            if n not in active[p]:
                continue
            path, isvar = path_for(n, p)
            variant[(p, n)] = isvar
            o, r = measure(cmd + [path], cpu)
            if o is None:
                notes[(p, n)] = "FAILED: " + str(r).strip().replace("\n", " ")
                active[p].remove(n)
                continue
            outputs[(p, n)] = o
            samples[(p, n)] = []
    for run in range(runs):
        print(f"run {run + 1}/{runs}", file=sys.stderr, flush=True)
        for p in progs:
            for n, cmd in bins:
                if n not in active[p]:
                    continue
                o, r = measure(cmd + [path_for(n, p)[0]], cpu)
                if o is not None:
                    samples[(p, n)].append(r)
    base = names[0]
    result = {"cpu": cpu, "runs": runs, "bins": {n: " ".join(c) for n, c in bins}, "baseline": base,
              "numeric_subset": NUMERIC, "noise_band": NOISE, "programs": {}}
    for p in progs:
        row = {}
        b0 = outputs.get((p, base))
        for n in names:
            s = samples.get((p, n))
            if not s:
                row[n] = {"note": notes.get((p, n), "excluded")}
                continue
            d = {}
            for ev in ("cycles", "instructions"):
                v = [x[ev] for x in s if ev in x]
                d[ev + "_min"] = min(v)
                d[ev + "_median"] = int(statistics.median(v))
            d["runs"] = len(s)
            d["variant"] = variant[(p, n)]
            d["output_matches_baseline"] = (outputs[(p, n)] == b0)
            exp = expected_for(p)
            if exp is not None:
                d["expected_ok"] = (outputs[(p, n)] == exp)
            row[n] = d
        result["programs"][p] = row
    # ratios and geomeans (vs baseline, min cycles and median cycles, plus instructions)
    result["ratios"] = {}
    result["geomean"] = {}
    for n in names[1:]:
        rat = {}
        for p in progs:
            a, b = result["programs"][p].get(base, {}), result["programs"][p].get(n, {})
            if "cycles_min" in a and "cycles_min" in b:
                rat[p] = {m: b[m] / a[m] for m in ("cycles_min", "cycles_median", "instructions_min", "instructions_median")}
        result["ratios"][n] = rat
        g = {}
        for m in ("cycles_min", "cycles_median", "instructions_min", "instructions_median"):
            g["all_" + m] = geomean([r[m] for r in rat.values()])
            g["numeric_" + m] = geomean([r[m] for q, r in rat.items() if q in NUMERIC])
        g["n_programs"] = len(rat)
        result["geomean"][n] = g
    if out_json:
        with open(out_json, "w") as f:
            json.dump(result, f, indent=1)
    print_table(result, names)


def print_table(res, names):
    base = names[0]
    hdr = f"{'program':<14}{base + ' Mcyc(min)':>16}" + "".join(f"{n + ' Mcyc':>14}{'ratio':>9}" for n in names[1:])
    print(hdr)
    for p, row in res["programs"].items():
        b = row.get(base, {})
        line = f"{p:<14}{b.get('cycles_min', 0) / 1e6:>16.1f}"
        for n in names[1:]:
            d = row.get(n, {})
            if "cycles_min" not in d:
                line += f"{'-':>14}{'-':>9}"
                continue
            r = res["ratios"][n].get(p, {}).get("cycles_min")
            flag = "~" if r is not None and abs(r - 1) <= NOISE else " "
            mark = "*" if d["variant"] else ""
            mm = "!" if not d["output_matches_baseline"] else ""
            mm += "X" if d.get("expected_ok") is False else ""
            line += f"{d['cycles_min'] / 1e6:>14.1f}{(f'{r:.3f}' if r else '-') + flag:>9}{mark}{mm}"
        print(line)
    for n in names[1:]:
        g = res["geomean"][n]
        print(f"geomean {n}: all({g['n_programs']}) cycles min {g['all_cycles_min']:.3f} median {g['all_cycles_median']:.3f}"
              f" instr {g['all_instructions_min']:.3f}; numeric cycles min {g['numeric_cycles_min']:.3f}"
              f" median {g['numeric_cycles_median']:.3f} instr {g['numeric_instructions_min']:.3f}")
    print("~ = within +-5% (noise); * = LuaJIT variant program; ! = stdout differs from baseline; X = differs from <prog>.expected")


if __name__ == "__main__":
    main()
