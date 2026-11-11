#!/usr/bin/env python3
"""C5: startup cost. min of N runs of `<cmd>` pinned to CPU 4: cycles:u, instructions:u (perf stat), wall ms, max RSS KB.
usage: startup.py <outjson> [N]"""
import json, os, subprocess, sys, tempfile, time, resource

OUT = sys.argv[1]
N = int(sys.argv[2]) if len(sys.argv) > 2 else 50
B = os.path.expanduser("~/.cache/vcBuild/bin")
R = "<repo>/ref"
TMP = os.path.expanduser("~/.cache/sb/tmp")
os.makedirs(TMP, exist_ok=True)
env0 = dict(os.environ, RUSTYNAIL_SEED="0")
SC = os.path.join(TMP, "sc")
os.makedirs(SC, exist_ok=True)
scripts = {
    "empty": "",
    "print": "print('hello')\n",
    "loop1k": "local s = 0 for i = 1, 1000 do s = s + i end print(s)\n",
    "fib20": "local function fib(n) if n < 2 then return n end return fib(n-1) + fib(n-2) end print(fib(20))\n",
    "tab_str": "local t = {} for i = 1, 200 do t[#t+1] = tostring(i) .. 'x' end print(#table.concat(t))\n",
}
for k, v in scripts.items():
    open(os.path.join(SC, k + ".lua"), "w").write(v)
subj = {
    "rn55plain": [f"{B}/p55/rnlua"], "rn55pgo": [f"{B}/g55/rnlua"],
    "rn55plain_jitoff": [f"{B}/p55/rnlua"],
    "rn55nojit-build": [f"{B}/n55/rnlua"], "rn54plain": [f"{B}/p54/rnlua"], "rn54pgo": [f"{B}/g54/rnlua"], "rn54nojit-build": [f"{B}/n54/rnlua"],
    "PUC55": [f"{R}/55/build-seed0/lua"], "PUC54": [f"{R}/54/build-seed0/lua"], "LuaJIT": ["/usr/bin/luajit"],
}


JOFF = {"rn55plain_jitoff"}
def one(cmd, n=None):
    with tempfile.NamedTemporaryFile("r", suffix=".perf", dir=TMP) as f:
        full = ["perf", "stat", "-x,", "-o", f.name, "-e", "cycles:u,instructions:u,cycles:k", "--", "taskset", "-c", "4"] + cmd
        t = time.perf_counter()
        r0 = resource.getrusage(resource.RUSAGE_CHILDREN)
        p = subprocess.run(full, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL, timeout=60, env=dict(env0, RUSTYNAIL_JIT="off") if n in JOFF else env0)
        w = time.perf_counter() - t
        res = {}
        for line in f.read().splitlines():
            fld = line.split(",")
            if len(fld) > 2 and fld[0].isdigit():
                res[fld[2]] = int(fld[0])
        return p.returncode, w, res


RSSLUA = os.path.join(SC, "rss.lua")
open(RSSLUA, "w").write("""local f = io.open('/proc/self/status') local t = f:read('a') f:close()
print(t:match('VmHWM:%s*(%d+)'), t:match('VmRSS:%s*(%d+)'), collectgarbage('count'))
""")
open(os.path.join(SC, "rss_fib.lua"), "w").write("""local function fib(n) if n < 2 then return n end return fib(n-1) + fib(n-2) end fib(22)
local f = io.open('/proc/self/status') local t = f:read('a') f:close()
print(t:match('VmHWM:%s*(%d+)'), t:match('VmRSS:%s*(%d+)'), collectgarbage('count'))
""")


def rss(cmd, script):
    p = subprocess.run(cmd + [os.path.join(SC, script)], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, stdin=subprocess.DEVNULL, env=env0)
    return p.stdout.decode().split()


subprocess.run(["taskset", "-c", "4", "python3", "-c", "import time\nt=time.time()\nwhile time.time()-t<0.5: pass"])
out = {}
names = list(subj)
for sname, sc in scripts.items():
    path = os.path.join(SC, sname + ".lua")
    # empty script via a file for all; `-e ''` also measured for "dash_e"
    rows = {n: {"cyc": [], "cyc_k": [], "ins": [], "wall": []} for n in names}
    for i in range(N):
        for k in range(len(names)):
            n = names[(i + k) % len(names)]
            rc, w, res = one(subj[n] + [path], n)
            if rc != 0:
                rows[n]["err"] = rc; continue
            rows[n]["cyc"].append(res.get("cycles:u")); rows[n]["ins"].append(res.get("instructions:u")); rows[n]["cyc_k"].append(res.get("cycles:k")); rows[n]["wall"].append(w)
    out[sname] = {n: {"cyc_min": min(v["cyc"]) if v["cyc"] else None, "ins_min": min(v["ins"]) if v["ins"] else None,
                      "cyc_k_min": min(x for x in v["cyc_k"] if x) if any(v["cyc_k"]) else None,
                      "wall_ms_min": min(v["wall"]) * 1000 if v["wall"] else None, "wall_ms_med": sorted(v["wall"])[len(v["wall"]) // 2] * 1000 if v["wall"] else None, "err": v.get("err")} for n, v in rows.items()}
    print(sname, {n: (out[sname][n]["cyc_min"], round(out[sname][n]["wall_ms_min"], 2) if out[sname][n]["wall_ms_min"] else None) for n in names}, flush=True)
# dash-e
rows = {n: {"cyc": [], "wall": []} for n in names}
for i in range(N):
    for k in range(len(names)):
        n = names[(i + k) % len(names)]
        rc, w, res = one(subj[n] + ["-e", ""], n)
        if rc == 0:
            rows[n]["cyc"].append(res.get("cycles:u")); rows[n]["wall"].append(w)
out["dash_e"] = {n: {"cyc_min": min(v["cyc"]) if v["cyc"] else None, "wall_ms_min": min(v["wall"]) * 1000 if v["wall"] else None} for n, v in rows.items()}
print("dash_e", {n: out["dash_e"][n]["cyc_min"] for n in names}, flush=True)
out["rss_kb"] = {n: {s: rss(subj[n] if n not in JOFF else ["env", "RUSTYNAIL_JIT=off"] + subj[n], s) for s in ("rss.lua", "rss_fib.lua")} for n in names}
print("rss", out["rss_kb"])
json.dump(out, open(OUT, "w"), indent=1)
