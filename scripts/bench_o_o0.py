#!/usr/bin/env python3
import glob, os, re, sys
D = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/.cache/ob/o0")
H = os.path.expanduser("~/.cache/ob/o0h")
rows = []
for f in sorted(glob.glob(D + "/*.err")):
    key = os.path.basename(f)[:-4].replace("__", "/")
    txt = open(f, errors="replace").read()
    m = re.search(r"\[trace\] exits (\d+)", txt)
    exits = int(m.group(1)) if m else 0
    m = re.search(r"entered (\d+)", txt)
    entered = int(m.group(1)) if m else 0
    call = callee = idx = maxside = shortb = untr = 0
    for l in txt.splitlines():
        if not l.startswith("[exitdump]"):
            continue
        g = re.search(r" n (\d+) pc (\d+) op Some\((\w+)\).* gk (\d+) st (.*?) fails (\d+) last (.*?) black", l)
        if not g:
            continue
        n, pc, op, gk = int(g.group(1)), int(g.group(2)), g.group(3), int(g.group(4))
        last = g.group(7)
        if op in ("SelfOp", "Call", "Tailcall"):
            call += n
        if gk & 1:
            callee += n
        if gk & 14 and not gk & 1 and op in ("SelfOp", "Getfield", "Gettable", "Gettabup", "Call", "Tailcall", "Eq", "Lt", "Le"):
            idx += n
        if "MaxSide" in last:
            maxside += n
        if "SideShort" in last:
            shortb += n
        if "SideUntraced" in last:
            untr += n
    # hook data
    w = {1: 0, 2: 0, 3: 0, 4: 0, 5: 0}
    sites = {2: 0, 3: 0, 4: 0, 5: 0}
    tot = 0
    hf = H + "/" + os.path.basename(f)
    if os.path.exists(hf):
        for l in open(hf, errors="replace"):
            p = l.split()
            if p and p[0] == "SITE":
                n, c = int(p[1]), int(p[2])
                tot += c
                k = min(n, 5)
                w[k] += c
                if n >= 2:
                    sites[k] += 1
    poly = sum(w[k] for k in (2, 3, 4, 5))
    rows.append((key, exits, entered, call, callee, idx, maxside, shortb, untr, tot, w, sites, poly))
print("| program | exits | entered | exits/entry | call-site pc exits | callee-guard exits | index-chain exits | MaxSide-blocked exits | SideShort-blocked exits | Lua calls seen | % calls at 2-4 callee sites | sites 2/3/4/5+ |")
print("|---|---|---|---|---|---|---|---|---|---|---|---|")
rows.sort(key=lambda r: -r[1])
for key, exits, entered, call, callee, idx, maxside, shortb, untr, tot, w, sites, poly in rows:
    if exits < 1000 and poly == 0:
        continue
    epe = exits / entered if entered else 0
    p24 = 100.0 * (w[2] + w[3] + w[4]) / tot if tot else 0
    print(f"| {key} | {exits} | {entered} | {epe:.1f} | {call} | {callee} | {idx} | {maxside} | {shortb} | {tot} | {p24:.1f} | {sites[2]}/{sites[3]}/{sites[4]}/{sites[5]} |")
