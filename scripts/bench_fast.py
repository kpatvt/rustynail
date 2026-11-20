#!/usr/bin/env python3
"""Cached-baseline benchmark driver (docs/GATES.md "Benchmarks"): measures only the SUBJECT binaries and takes the columns that do
not change (PUC, LuaJIT, rebuilt reference commits) from bench/results/baselines/, keyed by subject commit + machine.

  scripts/bench_fast.py run     (environment, see below; use scripts/bench-fast.sh: it adds the memory cap and the timeout)
  scripts/bench_fast.py import  --from JSON [--extra JSON ...] --col NAME --id ID --desc TEXT [--commit C] [--binary PATH] [--cpus 3-7]
  scripts/bench_fast.py status
  scripts/bench_fast.py parcheck --cpus 4,5,6 --cmd 'env RUSTYNAIL_SEED=0 /path/rnlua' [--programs set/prog,...] [--runs 5]

run (same variables as scripts/bench_session.py where they exist):
  CPUS=4        pinned worker CPUs (one worker per CPU). More than the proven parallelism of this machine
                (bench/results/baselines/parallel-check.json, written by `parcheck`) is refused unless PAR_FORCE=1.
  RUNS=5        runs per (program, column) of a program that needs the full measurement (min of RUNS, as session-nbody)
  QUICK_RUNS=2  every program first gets QUICK_RUNS interleaved runs of the subject columns (instruction counts are
                deterministic: the first run counts them, the second gives the cycle spread)
  INS_TOL=0.005 a program gets the full RUNS when a subject column's instruction count differs from its paired cached column by more
  CV_TOL=0.02   ... or when the cycle spread of the quick runs, max/min - 1, is above this ("noisy")
  ALL_FULL=1    every program gets the full RUNS (the cached baselines are still used)
  SETS, ONLY, OUT, SPIN_MS   as bench_session.py (SETS default: the 9 sets of session-nbody)
  COLS='name=cmd;name2=cmd'  SUBJECT columns (bench_session.py syntax: `env RUSTYNAIL_SEED=0 /path/rnlua`; env columns are padded
                             to the same environment length as the cached columns they are compared with)
  BASE='puc:puc55;lj:lj21;ljoff:ljoff21;b1p:b1p-108b935;b1g:b1g-108b935'   cached columns `name:cache-id[=cmd]` (cmd only for the
                             canary: a command that can still be run, e.g. puc:puc55=/path/lua)
  PAIR='plain=b1p;pgo=b1g'   which cached column the instruction counts of a subject column are compared with
  CANARY=3      programs re-measured with the cached columns that have a cmd (min of 5 runs) before the run: a median drift of more than
                CANARY_TOL (0.025), or a worst drift above CANARY_MAX (0.06), means the machine is not in the state of the cached run: the run stops (FORCE_STALE=1 continues)
Selection and stale checks print to stderr; the JSON is bench_session.py's schema plus `n` (runs), `flag` and `cached` per column and a `meta` block,
so scripts/bench_session_report.py-style readers work unchanged.
"""
import glob, hashlib, json, math, os, platform, queue, subprocess, sys, tempfile, threading, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
CACHE = os.path.join(ROOT, "bench/results/baselines")
MANIFEST = os.path.join(CACHE, "manifest.json")
PARFILE = os.path.join(CACHE, "parallel-check.json")
DEFAULT_SETS = "corpus,corpus2/dev,corpus2/heldout,awfy,programs,jit-trace,lib,apps,real"


def eprint(*a):
    print(*a, file=sys.stderr, flush=True)


# ---- machine --------------------------------------------------------------------------------------------------------------------
def machine():
    model = ""
    for line in open("/proc/cpuinfo"):
        if line.startswith("model name"):
            model = line.split(":", 1)[1].strip()
            break
    def rd(p):
        try:
            return open(p).read().strip()
        except OSError:
            return ""
    info = {"cpu": model, "nproc": os.cpu_count(), "kernel": platform.release(),
            "governor": rd("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor"),
            "boost": rd("/sys/devices/system/cpu/cpufreq/boost")}
    info["id"] = hashlib.sha1(f"{model}|{info['nproc']}".encode()).hexdigest()[:8]
    return info


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def sibling_of(cpu):
    try:
        s = open(f"/sys/devices/system/cpu/cpu{cpu}/topology/thread_siblings_list").read().strip()
    except OSError:
        return []
    out = []
    for part in s.split(","):
        if "-" in part:
            a, b = part.split("-")
            out += list(range(int(a), int(b) + 1))
        else:
            out.append(int(part))
    return [c for c in out if c != cpu]


def busy(cpus, dt=2.0):
    def snap():
        d = {}
        for l in open("/proc/stat"):
            f = l.split()
            if f[0].startswith("cpu") and f[0] != "cpu":
                v = list(map(int, f[1:9]))
                d[int(f[0][3:])] = (sum(v), v[3] + v[4])
        return d
    a = snap()
    time.sleep(dt)
    b = snap()
    return {c: 100 * (1 - (b[c][1] - a[c][1]) / max(1, b[c][0] - a[c][0])) for c in cpus}


# ---- manifest -------------------------------------------------------------------------------------------------------------------
def load_manifest():
    if os.path.exists(MANIFEST):
        return json.load(open(MANIFEST))
    return {"schema": 1, "entries": []}


def save_manifest(m):
    os.makedirs(CACHE, exist_ok=True)
    json.dump(m, open(MANIFEST, "w"), indent=1, sort_keys=True)
    open(MANIFEST, "a").write("\n")


def envlen_of(cmd):
    c = cmd.split()
    return sum(len(x) + 1 for x in c[1:] if "=" in x) if c and c[0] == "env" else 0


def cmd_import(args):
    opt = {"extra": []}
    i = 0
    while i < len(args):
        k = args[i]
        if k == "--extra":
            opt["extra"].append(args[i + 1])
        else:
            opt[k.lstrip("-")] = args[i + 1]
        i += 2
    src = json.load(open(opt["from"]))
    col, cid = opt["col"], opt["id"]
    mach = machine()
    res = {}
    for key, ent in src["results"].items():
        c = ent["cols"].get(col)
        if not c or not c.get("cyc"):
            continue
        res[key] = {"cyc": [x for x in c["cyc"] if x], "ins": [x for x in c["ins"] if x], "out": c.get("out", ""),
                    "exp": c.get("exp", "none"), "variant": c.get("variant", False), "cpu": ent.get("cpu")}
    for ex in opt["extra"]:
        e = json.load(open(ex))
        for key, ent in e["results"].items():
            c = ent["cols"].get(col)
            if c and c.get("cyc") and key in res:
                res[key]["cyc"] += [x for x in c["cyc"] if x]
                res[key]["ins"] += [x for x in c["ins"] if x]
                res[key].setdefault("merged_from", []).append(os.path.basename(ex))
    cmd = src["cols"].get(col, "")
    binary = opt.get("binary", "")
    entry = {"id": cid, "desc": opt.get("desc", ""), "commit": opt.get("commit", ""), "machine": mach["id"],
             "key": f"{cid}|{opt.get('commit') or (sha256(os.path.join(ROOT, binary))[:12] if binary and os.path.exists(os.path.join(ROOT, binary)) else '-')}|{mach['id']}",
             "file": f"{cid}-{mach['id']}.json", "binary": binary, "binary_sha256": sha256(os.path.join(ROOT, binary)) if binary and os.path.exists(os.path.join(ROOT, binary)) else "",
             "source": os.path.relpath(opt["from"], ROOT) if not os.path.isabs(opt["from"]) else opt["from"],
             "source_extra": opt["extra"], "source_col": col, "cmd_at_measure": cmd, "envlen": envlen_of(cmd),
             "cpus_at_measure": opt.get("cpus", ""), "runs_in_source": src.get("runs"), "programs": len(res),
             "method": "min of RUNS interleaved pinned runs, perf stat cycles:u,instructions:u (scripts/bench_session.py)",
             "machine_info": mach, "imported": time.strftime("%Y-%m-%d")}
    os.makedirs(CACHE, exist_ok=True)
    json.dump({"entry": {k: v for k, v in entry.items() if k != "machine_info"}, "results": res}, open(os.path.join(CACHE, entry["file"]), "w"))
    m = load_manifest()
    m["entries"] = [e for e in m["entries"] if not (e["id"] == cid and e["machine"] == mach["id"])] + [entry]
    save_manifest(m)
    print(f"imported {cid}: {len(res)} programs from {col} of {opt['from']} -> {entry['file']}")


def cmd_status(_):
    m = load_manifest()
    me = machine()
    print(f"this machine: {me['id']} {me['cpu']} x{me['nproc']} kernel {me['kernel']} governor {me['governor']}")
    for e in m["entries"]:
        ok = "this machine" if e["machine"] == me["id"] else "OTHER MACHINE"
        print(f"  {e['id']:<22} {e['programs']:>4} programs  {ok:<13} commit {e['commit'] or '-':<9} {e['desc']}")
    if os.path.exists(PARFILE):
        p = json.load(open(PARFILE))
        print(f"parallel check: {p.get('machine')} proven workers {p.get('proven_workers')} max drift {p.get('max_drift_pct')}%")


def load_cached(cid, mach, allow_other=False):
    m = load_manifest()
    cands = [e for e in m["entries"] if e["id"] == cid]
    if not cands:
        sys.exit(f"bench-fast: no cached baseline '{cid}' (scripts/bench_fast.py status; import one with `import`)")
    ok = [e for e in cands if e["machine"] == mach["id"]]
    if not ok and not allow_other:
        sys.exit(f"bench-fast: baseline '{cid}' was measured on another machine ({cands[0]['machine_info']['cpu']}): not used (FORCE_STALE=1)")
    e = (ok or cands)[0]
    # a baseline whose binary still exists must still be that binary (PUC rebuilt, LuaJIT upgraded: stale)
    bpath = os.path.join(ROOT, e["binary"]) if e.get("binary") else ""  # (an absolute path wins in join)
    if bpath and os.path.exists(bpath) and e.get("binary_sha256") and sha256(bpath) != e["binary_sha256"]:
        if not allow_other:
            sys.exit(f"bench-fast: baseline '{cid}' is stale: {e['binary']} changed since it was measured (FORCE_STALE=1)")
        eprint(f"bench-fast: WARNING baseline '{cid}' is stale ({e['binary']} changed)")
    data = json.load(open(os.path.join(CACHE, e["file"])))
    return e, data["results"]


# ---- measuring ------------------------------------------------------------------------------------------------------------------
def geomean(xs):
    xs = [x for x in xs if x and x > 0]
    return math.exp(sum(math.log(x) for x in xs) / len(xs)) if xs else float("nan")


def parse_cols(s):
    cols = []
    for item in s.split(";"):
        if item.strip():
            n, c = item.split("=", 1)
            cols.append((n.strip(), c.split()))
    return cols


def pad_env(cols, target):
    out = []
    for n, c in cols:
        if c and c[0] == "env":
            need = target - envlen_of(" ".join(c))
            if need < 9:
                # no room for RUSTYNAIL_PAD=<x>: the unpadded length is longer than the target
                out.append((n, c))
            else:
                out.append((n, ["env", "RUSTYNAIL_PAD=" + "x" * (need - len("RUSTYNAIL_PAD=") - 1)] + c[1:]))
        else:
            out.append((n, c))
    return out


def prog_path(col, s, p, skip):
    if col.startswith("lj"):
        for c in (os.path.join(ROOT, "bench/lj", s, p + ".lua"),
                  os.path.join(ROOT, "bench/luajit", p + ".lua") if s == "programs" else ""):
            if c and os.path.exists(c):
                return c
    return os.path.join(ROOT, "bench", s, p + ".lua")


def read_skip():
    skip = set()
    for line in open(os.path.join(ROOT, "bench/lj/SKIP")):
        line = line.split("#")[0].strip()
        if line:
            skip.add(line)
    return skip


def proven_workers(mach):
    if os.path.exists(PARFILE):
        p = json.load(open(PARFILE))
        if p.get("machine") == mach["id"]:
            return int(p.get("proven_workers", 1))
    return 1


def canary(base_cols, cached, mach, cpu, n_prog, tol, bs):
    """Re-measure a few programs with the cached columns that have a command; returns (median |drift|, worst |drift|, lines)
    of the min cycles (min of 5 here against the cached min of 5..15: a small positive bias is normal)."""
    skip = read_skip()
    keys = ["corpus/class_middleclass", "programs/nbody", "awfy/richards", "lib/sel_loop", "apps/events"]
    drifts, lines = [], []
    for name, (cid, cmd) in base_cols.items():
        if not cmd:
            continue
        done = 0
        for key in keys:
            s, p = key.rsplit("/", 1)
            if key not in cached[name] or (name.startswith("lj") and key in skip) or done >= n_prog:
                continue
            cyc = []
            for _ in range(5):
                rc, so, se, res = bs.measure(cmd.split() + [prog_path(name, s, p, skip)], cpu)
                if rc == 0:
                    cyc.append(res.get("cycles"))
            if not cyc:
                continue
            old = min(cached[name][key]["cyc"])
            d = min(cyc) / old - 1
            lines.append(f"  canary {name:<6} {key:<26} cached {old / 1e6:9.1f} Mcyc  now {min(cyc) / 1e6:9.1f}  drift {100 * d:+.2f}%")
            drifts.append(abs(d))
            done += 1
    drifts.sort()
    return (drifts[len(drifts) // 2] if drifts else 0.0), (drifts[-1] if drifts else 0.0), lines


def cmd_run(_):
    import bench_session as bs
    os.makedirs(bs.TMP, exist_ok=True)
    mach = machine()
    cpus = [int(x) for x in os.environ.get("CPUS", "4").split(",")]
    runs = int(os.environ.get("RUNS", "5"))
    quick = int(os.environ.get("QUICK_RUNS", "2"))
    ins_tol = float(os.environ.get("INS_TOL", "0.005"))
    cv_tol = float(os.environ.get("CV_TOL", "0.02"))
    all_full = os.environ.get("ALL_FULL", "0") == "1"
    force = os.environ.get("FORCE_STALE", "0") == "1"
    out = os.environ["OUT"]
    sets = [s for s in os.environ.get("SETS", DEFAULT_SETS).split(",") if s]
    only = set(x for x in os.environ.get("ONLY", "").split(",") if x)
    subj = parse_cols(os.environ["COLS"])
    pairs = dict(x.split("=") for x in os.environ.get("PAIR", "").split(";") if "=" in x)
    # cached columns
    base_cols, cached, entries = {}, {}, {}
    for item in os.environ.get("BASE", "").split(";"):
        if not item.strip():
            continue
        head, _, cmd = item.partition("=")
        name, _, cid = head.partition(":")
        e, res = load_cached(cid or name, mach, force)
        base_cols[name.strip()] = (cid or name, cmd.strip())
        cached[name.strip()] = res
        entries[name.strip()] = e
    # parallelism: only what the parallel check proved on this machine
    pw = proven_workers(mach)
    if len(cpus) > pw and os.environ.get("PAR_FORCE", "0") != "1":
        eprint(f"bench-fast: {len(cpus)} workers requested, only {pw} proven on this machine (scripts/bench_fast.py parcheck): using CPUS={','.join(map(str, cpus[:pw]))} (PAR_FORCE=1 to override)")
        cpus = cpus[:pw]
    sib = sorted({s for c in cpus for s in sibling_of(c)} - set(cpus))
    if sib:
        b = busy(sib + cpus, 2.0)
        hot = {c: round(v, 1) for c, v in b.items() if v > 5.0}
        eprint("bench-fast: idle check cpus " + " ".join(f"{c}:{b[c]:.1f}%" for c in sorted(b)))
        if hot and not force:
            sys.exit(f"bench-fast: busy CPUs among workers/SMT siblings: {hot} (FORCE_STALE=1 to run anyway)")
    # environment padding equal to the cached columns'
    target = max([envlen_of(e["cmd_at_measure"]) for e in entries.values()] + [envlen_of(" ".join(c)) for _, c in subj] + [0])
    subj = pad_env(subj, target)
    # canary
    n_can = int(os.environ.get("CANARY", "3"))
    if n_can > 0 and any(cmd for _, cmd in base_cols.values()):
        med, worst, lines = canary(base_cols, cached, mach, cpus[0], n_can, 0.03, bs)
        for l in lines:
            eprint(l)
        eprint(f"bench-fast: canary median |drift| {100 * med:.2f}%, worst {100 * worst:.2f}%")
        if (med > float(os.environ.get("CANARY_TOL", "0.025")) or worst > float(os.environ.get("CANARY_MAX", "0.06"))) and not force:
            sys.exit("bench-fast: canary drift above tolerance (median 2.5% / worst 6%): the machine is not in the state of the cached run (FORCE_STALE=1 to continue)")
    skip = read_skip()
    q = queue.Queue()
    for s in sets:
        for p in sorted(os.path.basename(x)[:-4] for x in glob.glob(os.path.join(ROOT, "bench", s, "*.lua"))):
            if only and f"{s}/{p}" not in only:
                continue
            q.put((s, p))
    total = q.qsize()
    results, lock, t0 = {}, threading.Lock(), time.time()
    stat = {"quick_only": 0, "ins": 0, "noisy": 0, "nobase": 0, "all_full": 0, "failed": 0}

    def measure_runs(cols, s, p, cpu, n, r, outs, start):
        for i in range(n):
            k = (start + i) % len(cols)
            for name, c in cols[k:] + cols[:k]:
                rc, so, se, res = bs.measure(c + [prog_path(name, s, p, skip)], cpu)
                e = r[name]
                if rc != 0:
                    e["err"] = f"rc{rc}: {se.strip()[-120:]}"
                    continue
                outs.setdefault(name, so)
                e["cyc"].append(res.get("cycles"))
                e["ins"].append(res.get("instructions"))

    def work(cpu):
        while True:
            try:
                s, p = q.get_nowait()
            except queue.Empty:
                return
            key = f"{s}/{p}"
            xp = os.path.join(ROOT, "bench", s, p + ".expected")
            exp = open(xp, "rb").read() if os.path.exists(xp) else None
            r = {n: {"cyc": [], "ins": [], "out": "", "exp": "none" if exp is None else "ok", "err": "", "variant": False} for n, _ in subj}
            outs = {}
            measure_runs(subj, s, p, cpu, quick, r, outs, 0)
            flags = set()
            for n, _ in subj:
                e = r[n]
                if e["err"] or not e["cyc"]:
                    flags.add("failed")
                    continue
                cv = max(e["cyc"]) / min(e["cyc"]) - 1
                if cv > cv_tol:
                    flags.add("noisy")
                pc = pairs.get(n)
                if pc is not None:
                    b = cached.get(pc, {}).get(key)
                    if not b or not b["ins"]:
                        flags.add("nobase")
                    elif abs(min(e["ins"]) / min(b["ins"]) - 1) > ins_tol:
                        flags.add("ins")
            if all_full:
                flags.add("all_full")
            if flags - {"failed"} and runs > quick:
                measure_runs(subj, s, p, cpu, runs - quick, r, outs, quick)
            ref = outs.get(subj[0][0])
            for n, _ in subj:
                o = outs.get(n)
                if o is None:
                    continue
                if exp is not None and o != exp:
                    r[n]["exp"] = "MISMATCH"
                r[n]["out"] = "same" if o == ref else "diff"
                r[n]["n"] = len(r[n]["cyc"])
                r[n]["flag"] = sorted(flags)
            with lock:
                for f in flags:
                    stat[f] += 1
                if not (flags - {"failed"}):
                    stat["quick_only"] += 1
                full = dict(r)
                for name, res in cached.items():
                    c = res.get(key)
                    if c:
                        full[name] = {"cyc": c["cyc"], "ins": c["ins"], "out": c.get("out", ""), "exp": c.get("exp", "none"),
                                      "err": "", "variant": c.get("variant", False), "cached": base_cols[name][0], "n": len(c["cyc"])}
                    elif name.startswith("lj") and key in skip:
                        full[name] = {"err": "skipped (no LuaJIT equivalent)"}
                results[key] = {"cpu": cpu, "cols": full}
                eprint(f"[{time.time() - t0:6.0f}s {len(results)}/{total} cpu{cpu}] {key} " + " ".join(
                    f"{n}={min(v['cyc']) / 1e6:.0f}({len(v['cyc'])})" if v.get("cyc") and not v.get("cached") else f"{n}=-" for n, v in r.items())
                    + (" [" + ",".join(sorted(flags)) + "]" if flags else ""))
                json.dump({"cols": {}, "runs": runs, "results": results}, open(out + ".part", "w"))

    ths = [threading.Thread(target=work, args=(c,)) for c in cpus]
    [t.start() for t in ths]
    [t.join() for t in ths]
    wall = time.time() - t0
    colcmds = {n: " ".join(c) for n, c in subj}
    for n, (cid, cmd) in base_cols.items():
        colcmds[n] = f"cached:{cid} ({entries[n]['cmd_at_measure']})"
    meta = {"driver": "bench_fast.py", "wall_s": round(wall), "machine": mach, "cpus": cpus, "runs": runs, "quick_runs": quick,
            "ins_tol": ins_tol, "cv_tol": cv_tol, "selection": stat, "programs": total,
            "baselines": {n: entries[n]["key"] for n in entries}, "pairs": pairs, "env_len": target}
    json.dump({"cols": colcmds, "runs": runs, "meta": meta, "results": results}, open(out, "w"), indent=1)
    os.remove(out + ".part")
    eprint(f"bench-fast: {total} programs in {wall:.0f}s; quick-only {stat['quick_only']}, full-run flags {stat}")
    for n, _ in subj:
        pc = pairs.get(n)
        if not pc:
            continue
        rc, ri = [], []
        for k, ent in results.items():
            a, b = ent["cols"].get(n, {}), ent["cols"].get(pc, {})
            if a.get("cyc") and b.get("cyc"):
                rc.append(min(a["cyc"]) / min(b["cyc"]))
                ri.append(min(a["ins"]) / min(b["ins"]))
        eprint(f"  geomean {n}/{pc}: cycles {geomean(rc):.4f}  instructions {geomean(ri):.4f}  ({len(rc)} programs)")


# ---- parallel check -------------------------------------------------------------------------------------------------------------
def cmd_parcheck(args):
    import bench_session as bs
    os.makedirs(bs.TMP, exist_ok=True)
    opt = {}
    for i in range(0, len(args), 2):
        opt[args[i].lstrip("-")] = args[i + 1]
    mach = machine()
    cpus = [int(x) for x in opt["cpus"].split(",")]
    cmd = opt["cmd"].split()
    progs = (opt.get("programs") or "corpus/class_middleclass,programs/nbody,awfy/richards,lib/sel_loop,apps/events,jit-trace/field_method").split(",")
    runs = int(opt.get("runs", "5"))
    skip = read_skip()
    sib = sorted({s for c in cpus for s in sibling_of(c)} - set(cpus))
    b = busy(sib + cpus, 2.0)
    eprint("parcheck: idle " + " ".join(f"{c}:{v:.1f}%" for c, v in sorted(b.items())))
    if any(v > 5 for v in b.values()):
        sys.exit("parcheck: CPUs or SMT siblings are busy: not a valid check")

    def one(cpu, key):
        s, p = key.rsplit("/", 1)
        c = []
        for _ in range(runs):
            rc, so, se, res = bs.measure(cmd + [prog_path("x", s, p, skip)], cpu)
            if rc == 0:
                c.append(res["cycles"])
        return min(c)
    serial = {key: one(cpus[0], key) for key in progs}
    serial2 = {key: one(cpus[0], key) for key in progs}  # the same measurement again: the noise floor of the method
    floor = max(abs(serial2[k] / serial[k] - 1) for k in progs)
    eprint(f"parcheck: noise floor (single worker measured twice, min of {runs}): max |drift| {100 * floor:.2f}%  " +
           " ".join(f"{k}={100 * (serial2[k] / serial[k] - 1):+.2f}" for k in progs))
    verdict = {"machine": mach["id"], "cpus": cpus, "siblings_idle": True, "programs": progs, "serial_cyc": serial,
               "serial_repeat_max_drift_pct": round(100 * floor, 2)}
    proven, worst_all = 1, 0.0
    for n in range(2, len(cpus) + 1):
        res, lock = {}, threading.Lock()

        def w(cpu):
            for key in progs:
                v = one(cpu, key)
                with lock:
                    res[(cpu, key)] = v
        ths = [threading.Thread(target=w, args=(c,)) for c in cpus[:n]]
        [t.start() for t in ths]
        [t.join() for t in ths]
        worst = 0.0
        det = {}
        for (cpu, key), v in sorted(res.items()):
            d = v / serial[key] - 1
            worst = max(worst, abs(d))
            det[f"cpu{cpu} {key}"] = round(100 * d, 2)
        verdict[f"parallel_{n}"] = {"max_abs_drift_pct": round(100 * worst, 2), "per": det}
        eprint(f"parcheck: {n} workers: max |drift| vs single {100 * worst:.2f}%  " + " ".join(f"{k}={v:+.2f}" for k, v in det.items()))
        worst_all = max(worst_all, worst)
        if worst <= 0.01 and proven == n - 1:
            proven = n  # (contiguous: n workers are proven only when every smaller count was too; larger counts are still measured and reported)
    verdict["proven_workers"] = proven
    verdict["max_drift_pct"] = round(100 * worst_all, 2)
    verdict["date"] = time.strftime("%Y-%m-%d")
    os.makedirs(CACHE, exist_ok=True)
    json.dump(verdict, open(opt.get("out", PARFILE), "w"), indent=1)
    eprint(f"parcheck: proven workers {proven} (tolerance 1%)")


if __name__ == "__main__":
    sub = sys.argv[1] if len(sys.argv) > 1 else "run"
    {"run": cmd_run, "import": cmd_import, "status": cmd_status, "parcheck": cmd_parcheck}[sub](sys.argv[2:])
