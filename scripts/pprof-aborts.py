#!/usr/bin/env python3
"""Trace statistics of every program of directories: recorded / installed / entered and the abort reasons
(RUSTYNAIL_JIT_VERBOSE=1 stderr). Summary: how many programs lose their main loop to which abort reason.
usage: pprof-aborts.py LUA_CLI dir [dir ...]   (run under systemd-run + timeout)"""
import collections, glob, os, re, subprocess, sys

lua = sys.argv[1]
tot = collections.Counter()
for d in sys.argv[2:]:
    for p in sorted(glob.glob(os.path.join(d, "*.lua"))):
        if os.path.basename(p).startswith("mod_"):
            continue
        r = subprocess.run([lua, p], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                           env=dict(os.environ, RUSTYNAIL_SEED="0", RUSTYNAIL_JIT_VERBOSE="1"), timeout=300)
        err = r.stderr.decode(errors="replace")
        m = re.search(r"\[trace\] recorded (\d+) installed (\d+) entered (\d+)", err)
        ab = re.findall(r"^\[trace\] aborted (\S+): (\d+)", err, re.M)
        rec, inst, ent = (int(x) for x in m.groups()) if m else (0, 0, 0)
        for reason, n in ab:
            tot[reason] += 1  # programs having this abort
        print(f"{os.path.basename(d)}/{os.path.basename(p)[:-4]:24} recorded {rec:3} installed {inst:3} entered {ent:10} aborts: "
              + ", ".join(f"{a}x{b}" for a, b in ab), flush=True)
print("programs with at least one abort of reason:", dict(tot.most_common()))
