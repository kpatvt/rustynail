import json, sys
D = sys.argv[1]
R = json.load(open(D + "/rerun55_serial_cpu4_15runs.json"))["results"]
v = lambda k, c, f: min([x for x in R[k]["cols"].get(c, {}).get(f, []) if x] or [0]) or None
for k in sorted(R):
    out = [k]
    for a, b in (("plain", "b9p"), ("pgo", "b9g"), ("plain", "b8p"), ("pgo", "b8g")):
        x, y = v(k, a, "cyc"), v(k, b, "cyc"); xi, yi = v(k, a, "ins"), v(k, b, "ins")
        out.append(f"{a}/{b} {x/y:.3f} (ins {xi/yi:.3f})")
    out.append(f"dup {v(k,'plain','cyc')/v(k,'plain2','cyc'):.3f}")
    print(" | ".join(out))
