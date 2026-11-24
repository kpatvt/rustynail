#!/usr/bin/env python3
"""Formation events of one run (docs/STUDY_FORMATION.md section 3): counts, outcomes and the duration of each formation attempt.

  scripts/formation-events.py ours LOG [LOG...] --ratio R      our scratch event log (tsc lines: BEGIN, START R|S .., OK .., F .. reason);
                                                              R = core cycles per TSC tick (measured with a busy loop under perf stat)
  scripts/formation-events.py lj ERR [ERR...] --ghz G         stderr of scripts/formation-lj.lua (seconds of CPU time; G = cycles per ns
                                                              from perf stat cycles / task-clock of the same program)
  scripts/formation-events.py blin LOG OUT                     the keys that never installed a trace in LOG (input of the a-priori blacklist)

An attempt runs from its START (ours: `begin_recording` / `begin_side_recording`; LuaJIT: `start`) to its end (ours: install = OK, or
`finish_abort` = F; LuaJIT: `stop` / `abort`). It includes the interpreter executing the recorded bytecodes, the recorder and the compile.
Several logs are averaged (counts and sums per run).
"""
import collections, sys


def med(v):
    v = sorted(v)
    return v[len(v) // 2] if v else 0.0


def ours(paths, ratio):
    agg = collections.defaultdict(list)
    for path in paths:
        lines = [l.split(None, 1) for l in open(path) if l.strip()]
        t0 = int(lines[0][0])
        cur = None
        att = []  # (kind, outcome, reason, start, dur)
        for t, rest in lines:
            t = int(t)
            w = rest.split()
            if w[0] == "START":
                cur = (w[1], t)
            elif w[0] in ("OK", "F") and cur is not None:
                kind = w[1]
                reason = w[-1] if w[0] == "F" else "ok"
                att.append((kind, w[0], reason, (cur[1] - t0) * ratio / 1e6, (t - cur[1]) * ratio / 1e6))
                cur = None
        agg["att"].append(att)
    report(agg["att"])


def lj(paths, ghz):
    runs = []
    for path in paths:
        att = []
        open_ = {}
        for l in open(path):
            w = l.split()
            if not w or w[0] == "END":
                continue
            t, what, tr, a, b = float(w[0]), w[1], w[2], w[3], w[4]
            cyc = t * ghz * 1e3  # Mcyc
            if what == "start":
                open_[tr] = (("S" if a != "-1" else "R"), cyc)
            elif what in ("stop", "abort") and tr in open_:
                kind, c0 = open_.pop(tr)
                att.append((kind, "OK" if what == "stop" else "F", "ok" if what == "stop" else "abort" + a, c0, cyc - c0))
        runs.append(att)
    report(runs)


def report(runs):
    n = len(runs)
    print(f"runs {n}")
    print("| kind | outcome | attempts / run | Mcyc / run (sum of durations) | median kcyc / attempt |")
    print("|---|---|---:|---:|---:|")
    keys = sorted({(a[0], a[1]) for r in runs for a in r})
    tot = 0.0
    for k in keys:
        ds = [a[4] for r in runs for a in r if (a[0], a[1]) == k]
        tot += sum(ds) / n
        print(f"| {k[0]} | {k[1]} | {len(ds) / n:.1f} | {sum(ds) / n:.2f} | {1000 * med(ds):.0f} |")
    print(f"total attempt time {tot:.2f} Mcyc / run")
    rs = collections.Counter(a[2] for r in runs for a in r if a[1] == "F")
    rd = collections.defaultdict(float)
    for r in runs:
        for a in r:
            if a[1] == "F":
                rd[a[2]] += a[4]
    print("failures by reason (attempts / run, Mcyc / run):", ", ".join(f"{k} {v / n:.1f} ({rd[k] / n:.2f})" for k, v in rs.most_common()))
    oks = sorted(a[3] + a[4] for a in runs[0] if a[1] == "OK")
    if oks:
        q = [oks[int(len(oks) * f) - 1 if f < 1 else -1] for f in (0.25, 0.5, 0.75, 0.9, 1.0)]
        print("installs of run 0 (Mcyc from start): 25 % at {:.1f}, 50 % at {:.1f}, 75 % at {:.1f}, 90 % at {:.1f}, last at {:.1f}".format(*q))


def blin(path, out):
    ok, fail = set(), collections.Counter()
    for l in open(path):
        w = l.split()
        if len(w) < 3:
            continue
        if w[1] == "OK":
            ok.add(" ".join(w[2:]))
        elif w[1] == "F":
            fail[" ".join(w[2:-1])] += 1
    keys = [k for k in fail if k not in ok]
    open(out, "w").write("".join(k + "\n" for k in keys))
    print(f"{len(keys)} keys never installed ({sum(fail[k] for k in keys)} of {sum(fail.values())} failed attempts)")


def main():
    a = sys.argv[1:]
    if a[0] == "ours":
        i = a.index("--ratio")
        ours(a[1:i], float(a[i + 1]))
    elif a[0] == "lj":
        i = a.index("--ghz")
        lj(a[1:i], float(a[i + 1]))
    elif a[0] == "blin":
        blin(a[1], a[2])


if __name__ == "__main__":
    main()
