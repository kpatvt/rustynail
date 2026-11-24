#!/usr/bin/env python3
"""Tables for docs/STUDY_CAUSAL_GAP.md from scripts/causal-grid.py output.

  scripts/causal-table.py grid OUT.json [base]     ablation matrix: median Mcyc of the base, then % delta per configuration
  scripts/causal-table.py ev OUT.json [OUT2 ...]   pipeline / event table per configuration (merges several event groups)
  scripts/causal-table.py reps OUT.json            in-process repetitions: first, second, third repetition cost
"""
import json, sys


def med(v):
    v = sorted(v)
    n = len(v)
    return (v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2) if v else float("nan")


def short(p):
    return p.split("/")[-1]


def grid(path, base=None, ev="cycles:u"):
    d = json.load(open(path))
    cfgs = [c["name"] for c in d["spec"]["configs"]]
    base = base or cfgs[0]
    print("| program | " + base + " Mcyc | " + " | ".join(c for c in cfgs if c != base) + " |")
    print("|---|" + "---:|" * len(cfgs))
    for p, r in d["results"].items():
        b = med(r[base]["ev"][ev])
        cells = []
        for c in cfgs:
            if c == base:
                continue
            v = med(r[c]["ev"].get(ev, []))
            flag = "" if r[c]["out"] == "ok" else " (" + r[c]["out"] + ")"
            cells.append(f"{100 * (v / b - 1):+.0f}{flag}")
        print(f"| {short(p)} | {b / 1e6:.1f} | " + " | ".join(cells) + " |")


def evtab(paths):
    res = {}
    for path in paths:
        d = json.load(open(path))
        for p, r in d["results"].items():
            for c, x in r.items():
                m = res.setdefault(p, {}).setdefault(c, {})
                for e, v in x["ev"].items():
                    if e == "cycles:u" and e in m:
                        continue
                    m[e] = med(v)
    hdr = ["Mcyc", "Minstr", "IPC", "retire%", "frontend%", "fe-lat%", "backend%", "badspec%", "brmisp/ki", "indmisp/ki",
           "icmiss/ki", "stli/ki", "ldwait%", "l2miss/ki"]
    print("| program | eng | " + " | ".join(hdr) + " |")
    print("|---|---|" + "---:|" * len(hdr))
    for p, cs in res.items():
        for c, m in cs.items():
            cyc, ins = m["cycles:u"], m.get("instructions:u", float("nan"))
            sl = 8 * cyc
            ki = ins / 1000
            g = lambda e: m.get(e, float("nan"))
            row = [cyc / 1e6, ins / 1e6, ins / cyc, 100 * g("ex_ret_ops:u") / sl,
                   100 * g("de_no_dispatch_per_slot.no_ops_from_frontend:u") / sl,
                   100 * g("cpu/de_no_dispatch_per_slot.no_ops_from_frontend") / cyc,
                   100 * g("de_no_dispatch_per_slot.backend_stalls:u") / sl,
                   100 * (g("de_src_op_disp.all:u") - g("ex_ret_ops:u")) / sl,
                   g("ex_ret_brn_misp:u") / ki, g("ex_ret_brn_ind_misp:u") / ki,
                   g("ic_tag_hit_miss.instruction_cache_miss:u") / ki, g("ls_bad_status2.stli_other:u") / ki,
                   100 * g("ex_no_retire.load_not_complete:u") / cyc, g("l2_cache_req_stat.ic_dc_miss_in_l2:u") / ki]
            print(f"| {short(p)} | {c} | " + " | ".join(f"{x:.2f}" if abs(x) < 10 else f"{x:.1f}" for x in row) + " |")


def reps(path):
    d = json.load(open(path))
    print("| program | eng | 1 rep Mcyc | 2nd rep | 3rd rep | warm-up = 1st - 3rd | warm-up share | 1 rep Minstr | 2nd rep | 3rd rep |")
    print("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|")
    for p, r in d["results"].items():
        for e in ("lj", "rn"):
            c = [med(r[f"{e}{k}"]["ev"]["cycles:u"]) / 1e6 for k in (1, 2, 3)]
            i = [med(r[f"{e}{k}"]["ev"]["instructions:u"]) / 1e6 for k in (1, 2, 3)]
            r2, r3 = c[1] - c[0], c[2] - c[1]
            w = c[0] - r3
            print(f"| {short(p)} | {e} | {c[0]:.1f} | {r2:.1f} | {r3:.1f} | {w:.1f} | {100 * w / c[0]:.0f} % | {i[0]:.1f} | "
                  f"{i[1] - i[0]:.1f} | {i[2] - i[1]:.1f} |")


if __name__ == "__main__":
    k = sys.argv[1]
    if k == "grid":
        grid(sys.argv[2], sys.argv[3] if len(sys.argv) > 3 else None, *(sys.argv[4:5]))
    elif k == "ev":
        evtab(sys.argv[2:])
    else:
        reps(sys.argv[2])
