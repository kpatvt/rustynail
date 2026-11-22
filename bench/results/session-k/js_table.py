#!/usr/bin/env python3
"""js_vs_node.md: ours (target/release/rnlua, JIT on) vs node vs LuaJIT, from js/raw.tsv and js20/raw.tsv, worst ours/node cycle ratio first."""
import csv, math
def load(p):
    d = {}
    for r in csv.DictReader(open(p), delimiter="\t"):
        if r["status"] == "ok":
            d.setdefault(r["name"], {})[r["engine"]] = (int(r["cycles"]), int(r["instr"]), float(r["wall_s"]))
    return d
gm = lambda xs: math.exp(sum(math.log(x) for x in xs) / len(xs))
out = []
for title, p in (("Default workloads (run.sh)", "js/raw.tsv"), ("20x workloads (run.sh -s 20; the 14 short programs)", "js20/raw.tsv")):
    d = load(p); empty = d.pop("EMPTY")["node"]
    rows = []
    for n, e in d.items():
        r, nd = e["rnlua"], e["node"]; lj = e.get("luajit")
        rows.append((r[0] / nd[0], n, r, nd, lj))
    rows.sort(reverse=True)
    out.append(f"## {title}\n\nEmpty node script: {empty[0]/1e6:.0f} Mcyc, {empty[2]*1000:.0f} ms (inside every node number). Ratios of cycles unless marked wall; above 1 = ours slower.\n")
    out.append("| program | ours Mcyc | node Mcyc | LuaJIT Mcyc | ours/node cyc | ours/node-ex cyc | ours/node wall | ours/LuaJIT cyc | ours ms | node ms | LuaJIT ms |")
    out.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
    for q, n, r, nd, lj in rows:
        out.append(f"| {n} | {r[0]/1e6:.0f} | {nd[0]/1e6:.0f} | {'n/a' if not lj else round(lj[0]/1e6)} | {q:.2f} | {r[0]/(nd[0]-empty[0]):.2f} | {r[2]/nd[2]:.2f} | {'n/a' if not lj else format(r[0]/lj[0], '.2f')} | {r[2]*1000:.0f} | {nd[2]*1000:.0f} | {'n/a' if not lj else round(lj[2]*1000)} |")
    out.append(f"\nGeomean ours/node: cycles {gm([x[0] for x in rows]):.2f}, wall {gm([x[2][2]/x[3][2] for x in rows]):.2f} ({len(rows)} programs); ours/LuaJIT cycles {gm([x[2][0]/x[4][0] for x in rows if x[4]]):.2f} ({sum(1 for x in rows if x[4])} programs).\n")
open("js_vs_node.md", "w").write("# JS ports: ours vs node vs LuaJIT (subject 5a064d7, 5.5 dev release build, JIT on)\n\nnode v26.13.1, min of 5 runs, taskset CPU 4, perf stat cycles:u. All 33 JS ports print exactly what the Lua programs print (run.sh checks before timing, 0 mismatches in both runs).\n\n" + "\n".join(out))
print("\n".join(out))
