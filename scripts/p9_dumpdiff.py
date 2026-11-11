#!/usr/bin/env python3
"""J5 P9: after /tmp/.../dumpcmp.sh-style dumps (<dir>/{A,B}/<prog>/proto-*.bin.txt = objdump of each dumped proto,
movabs immediates masked), pairs the protos of A and B by dump order, normalises away addresses and branch targets,
and reports the set of distinct instruction lines that appear on only one side (with counts), over all programs.
   scripts/p9_dumpdiff.py <dir>"""
import collections, difflib, glob, os, re, sys

d = sys.argv[1]
only_a, only_b = collections.Counter(), collections.Counter()
npairs = ndiff = 0
for prog in sorted(os.listdir(d + "/A")):
    if not os.path.isdir(f"{d}/A/{prog}"):
        continue
    fa = sorted(glob.glob(f"{d}/A/{prog}/*.bin.txt"))
    fb = sorted(glob.glob(f"{d}/B/{prog}/*.bin.txt"))
    if len(fa) != len(fb):
        print(f"{prog}: proto count differs {len(fa)} vs {len(fb)}")
        continue

    def norm(p):
        out = []
        for l in open(p):
            l = re.sub(r"^ *[0-9a-f]+:\t", "", l.rstrip("\n"))
            out.append(re.sub(r"0x[0-9a-f]+$", "T", l))
        return out

    for a, b in zip(fa, fb):
        npairs += 1
        na, nb = norm(a), norm(b)
        if na == nb:
            continue
        ndiff += 1
        sm = difflib.SequenceMatcher(None, na, nb, autojunk=False)
        for tag, i1, i2, j1, j2 in sm.get_opcodes():
            if tag != "equal":
                only_a.update(na[i1:i2])
                only_b.update(nb[j1:j2])
print(f"{npairs} proto pairs, {ndiff} differ after removing addresses and branch targets")
print("lines only in A:", dict(only_a.most_common(30)))
print("lines only in B:", dict(only_b.most_common(30)))
