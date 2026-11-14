import json
out = []
for f in ("startup_hyperfine.json", "startup_hyperfine_jitoff.json"):
    for r in json.load(open(f))["results"]:
        w = r["summary"]["time_wall_clock"]; m = r["summary"]["memory_peak_resident"]
        out.append(f"{r['name']:<20} wall min {w['min']*1000:.3f} median {w['median']*1000:.3f} mean {w['mean']*1000:.3f} ms | peak RSS {m['mean']/1e6:.2f} MB")
open("startup_hyperfine_summary.txt", "w").write("\n".join(out) + "\n")
print("\n".join(out))
