import re
s = open("report.py").read()
s = s.replace("bplain", "b9p").replace("bpgo", "b9g")
s = s.replace("session-bm5 (HEAD 9a9b8ed, rnlua", "session-final (HEAD 108b935, rnlua").replace("session-bm5 tables", "session-final tables")
s = s.replace("pass A: puc lj plain plain2(dup) int nj pgo b9p b9g", "pass A: puc lj ljoff plain plain2(dup) int nj pgo b9p b9g b8p b8g")
s = s.replace("base = baseline 8a18eb3 rebuilt same session (plain / PGO; squashed equivalent of session-release HEAD 8f422153).",
              "base = 9a9b8ed (bm5 subject) rebuilt same session (plain / PGO); base8 = 8a18eb3 (previous release table baseline) rebuilt same session.")
# C2 table: add base8 columns
s = s.replace("| PGO/b9g cyc | ins | PGO-training-set |\")", "| PGO/b9g cyc | ins | plain/b8p cyc | ins | PGO/b8g cyc | ins | PGO-training-set |\")")
s = s.replace('P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")\n    trained', 'P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")\n    trained')
s = s.replace("f\"{f3(G(A,ks,'pgo','b9g'))} | {f3(G(A,ks,'pgo','b9g',ins))} | {'trained'",
              "f\"{f3(G(A,ks,'pgo','b9g'))} | {f3(G(A,ks,'pgo','b9g',ins))} | {f3(G(A,ks,'plain','b8p'))} | {f3(G(A,ks,'plain','b8p',ins))} | {f3(G(A,ks,'pgo','b8g'))} | {f3(G(A,ks,'pgo','b8g',ins))} | {'trained'")
# per-prog: add LJoff and b8
s = s.replace("| PUC | LJ | pgo/plain | ip | dup | plain/PUC | plain/int | base | baseG | core |\")", "| PUC | LJ | LJoff | pgo/plain | ip | dup | plain/PUC | plain/int | base9 | baseG9 | base8 | baseG8 | core |\")")
s = s.replace('P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")\n        mc =', 'P("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")\n        mc =')
s = s.replace("{mc(k,'puc')} | {mc(k,'lj')} | {f3(ratio(R,k,'pgo','plain'))}", "{mc(k,'puc')} | {mc(k,'lj')} | {mc(k,'ljoff')} | {f3(ratio(R,k,'pgo','plain'))}")
s = s.replace("{f3(ratio(R,k,'pgo','b9g'))} | {R[k]['cpu']} |", "{f3(ratio(R,k,'pgo','b9g'))} | {f3(ratio(R,k,'plain','b8p'))} | {f3(ratio(R,k,'pgo','b8g'))} | {R[k]['cpu']} |")
# LJ tables: more columns
s = s.replace('[("plain", "plain"), ("PGO", "pgo"), ("base plain", "b9p"), ("base PGO", "b9g")]', '[("plain", "plain"), ("PGO", "pgo"), ("9a9 plain", "b9p"), ("9a9 PGO", "b9g"), ("8a1 plain", "b8p"), ("8a1 PGO", "b8g")]')
s = s.replace("| bplain/LJ | bpgo/LJ |", "| b9p/LJ | b9g/LJ | b8p/LJ | b8g/LJ |")
s = s.replace("PGO/LJ | b9p/LJ | b9g/LJ |\"); P(\"|---|---|---|---|---|---|---|---|---|---|---|---|---|\")", "PGO/LJ | b9p/LJ | b9g/LJ | b8p/LJ | b8g/LJ |\"); P(\"|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\")")
s = s.replace("{f3(ratio(B,k,'b9p','lj'))} | {f3(ratio(B,k,'b9g','lj'))} |", "{f3(ratio(B,k,'b9p','lj'))} | {f3(ratio(B,k,'b9g','lj'))} | {f3(ratio(B,k,'b8p','lj'))} | {f3(ratio(B,k,'b8g','lj'))} |")
s = s.replace('cn = ["puc", "lj", "plain", "plain2", "int", "nj", "pgo", "b9p", "b9g"]', 'cn = ["puc", "lj", "ljoff", "plain", "plain2", "int", "nj", "pgo", "b9p", "b9g", "b8p", "b8g"]')
s = s.replace('["puc", "lj", "plain", "pgo", "b9p", "b9g"]', '["puc", "lj", "plain", "pgo", "b9p", "b9g", "b8p", "b8g"]')
s = s.replace("Trend vs baseline 8a18eb3 (same session", "Trend vs 9a9b8ed (same session")
open("report.py", "w").write(s)
