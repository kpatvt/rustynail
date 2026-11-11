#!/usr/bin/env python3
"""usage: cpu_idle.py cpu,cpu,... [seconds]  -- print busy% of the given CPUs over the interval (from /proc/stat)."""
import sys, time
cpus = [int(x) for x in sys.argv[1].split(",")]
dt = float(sys.argv[2]) if len(sys.argv) > 2 else 3


def snap():
    d = {}
    for l in open("/proc/stat"):
        f = l.split()
        if f[0].startswith("cpu") and f[0] != "cpu":
            v = list(map(int, f[1:9])); d[int(f[0][3:])] = (sum(v), v[3] + v[4])
    return d


a = snap(); time.sleep(dt); b = snap()
print("  ".join(f"cpu{c}:{100 * (1 - (b[c][1] - a[c][1]) / max(1, b[c][0] - a[c][0])):.1f}%busy" for c in cpus))
