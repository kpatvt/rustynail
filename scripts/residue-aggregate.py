#!/usr/bin/env python3
"""Classify a program's tier-1 instructions by the reason the code is not in a trace (docs/STUDY_TIER1_RESIDUE.md).

  scripts/residue-aggregate.py DIR [DIR...] [--suite]

Each DIR holds `a.json` written by scripts/tier1-residue.py (categories, tier-1 share per prototype line, the exits that land in
tier 1 with their landing prototype, the head table). Attribution (an estimate, the shares are measured):

* a prototype's tier-1 share is split over the tier-1-landing exits whose landing prototype it is, in proportion to their exit
  counts (a prototype without head rows and fewer than 50000 such exits is `no head`: it runs where its tier-1 caller runs);
  each exit is classed by the abort that closed it (`last`: LeaveFrame at a RETURN / at a TAILCALL, MaxSide, SideShort,
  Nyi / CFunc, Depth / Recursion, TooLong, InnerLoop / InnerFamily, TailCall), `no-side` (blacklisted at install, `fails 0`: the start
  snapshot of a down-recursion `rec_ret` trace, `no_start_side`, or an exit whose cell is chained to a prelude) or `cold` (never attempted: below the hot-exit count);
* a prototype that no tier-1-landing exit enters is classed by its own head rows: the function-entry head's state and its most
  frequent abort (FnShort, InnerLoop, ...), `no head` (a function without a `CALL` has no function-entry head: it runs where its
  caller runs), or `head traced` (its trace exists but this code ran outside it: reached from tier-1 callers).

Prints one table per program and, with --suite, the sum over all DIRs weighted by each program's tier-1 share (each program
counts as 1, so the suite line is the mean share per program).
"""
import json
import re
import sys
from collections import defaultdict

args = sys.argv[1:]
suite = "--suite" in args
dirs = [a for a in args if a != "--suite"]


def exit_class(e):
    last = e["last"] or "None"
    if e["st"] == "Black" and e["fails"] == 0 and last == "None":
        return "exit:no-side (rec-ret start / chained entry)"
    if e["st"] in ("Free", "Wait") and last == "None":
        return "exit:cold"
    if last.startswith("LeaveFrame"):
        return "exit:LeaveFrame@Tailcall" if e["op"] == "Tailcall" else "exit:LeaveFrame@Return"
    for k, c in (
        ("MaxSide", "exit:MaxSide"),
        ("SideShort", "exit:SideShort"),
        ("Nyi", "exit:Nyi/CFunc"),
        ("CFunc", "exit:Nyi/CFunc"),
        ("Depth", "exit:Depth/Recursion"),
        ("Recursion", "exit:Depth/Recursion"),
        ("TooLong", "exit:TooLong"),
        ("Inner", "exit:InnerLoop"),
        ("TailCall", "exit:TailCall"),
        ("ChainDepth", "exit:other"),
    ):
        if last.startswith(k):
            return c
    return "exit:other(" + last + ")"


def head_class(line, heads):
    rows = [r for r in heads if re.search(rf":{line} pc \d+ ", r)]
    if not rows:
        return "head:no head"
    fn = [r for r in rows if re.search(r" pc 0 fn ", r)]
    lp = [r for r in rows if " loop " in r]
    pick = fn[0] if fn else (lp[0] if lp else rows[0])
    m = re.search(r" (fn|loop|cont) (\w+):", pick)
    state = m[2] if m else "?"
    if state == "Traced":
        return "head:head traced (reached from tier-1 callers)"
    ab = re.search(r" aborts \{([^}]*)\}", pick)
    if ab:
        cnt = [(int(x.split(":")[-1]), x.rsplit(":", 1)[0]) for x in ab[1].split(",") if ":" in x]
        cnt.sort(reverse=True)
        top = cnt[0][1] if cnt else "?"
        if top == "StartRefused":
            sr = re.search(r"start-refused \{([^}:]*)", pick)
            top += "(" + (sr[1] if sr else "?") + ")"
        return f"head:{top}"
    if "flushes" in pick:
        return "head:flushed"
    return f"head:{state} (no recording)"


tot = defaultdict(float)
nprog = 0
for d in dirs:
    try:
        j = json.load(open(f"{d}/a.json"))
    except Exception:
        continue
    nprog += 1
    t1 = {int(k[1:]): v for k, v in j["tier1"].items()}
    by_land = defaultdict(list)
    for e in j["exits"]:
        if e.get("land") is not None:
            by_land[e["land"]].append(e)
    cls = defaultdict(float)
    sites = []
    for line, share in t1.items():
        es = [e for e in by_land.get(line, []) if e["n"] > 0]
        n = sum(e["n"] for e in es)
        # exits with fewer than 1000 executions (or cold ones) cannot explain a visible share: class by the head rows then
        headless = not any(re.search(rf":{line} pc \d+ ", r) for r in j["heads"])
        if n >= 1000 and not (headless and n < 50000):
            for e in es:
                c = exit_class(e)
                cls[c] += share * e["n"] / n
            top = max(es, key=lambda e: e["n"])
            sites.append((share, line, f"{n} exits, top {exit_class(top)} (trace {top['trace']} {'side' if top['side'] else 'root'} of L{top['head'][0]} pc {top['head'][1]}, pc {top['pc']} {top['op']})"))
        else:
            c = head_class(line, j["heads"])
            cls[c] += share
            sites.append((share, line, c))
    t1sum = sum(t1.values())
    cats = j["cats"]
    name = d.rstrip("/").split("/")[-1]
    if not suite:
        print(f"== {name}: tier1 {t1sum:.1f}% (tier-1 helpers {cats.get('tier1-helpers', 0):.1f}, interp {cats.get('interp', 0):.1f})")
        for c, v in sorted(cls.items(), key=lambda x: -x[1]):
            if v >= 0.1:
                print(f"   {v:5.1f}%  {c}")
        for share, line, why in sorted(sites, reverse=True)[:6]:
            print(f"      L{line} {share:.1f}%: {why}")
    for c, v in cls.items():
        tot[c] += v
    tot["_tier1"] += t1sum
if suite and nprog:
    print(f"suite: {nprog} programs, mean tier-1 share {tot['_tier1'] / nprog:.2f}%")
    for c, v in sorted(tot.items(), key=lambda x: -x[1]):
        if c != "_tier1" and v / nprog >= 0.01:
            print(f"   {v / nprog:6.2f}%  {c}")
