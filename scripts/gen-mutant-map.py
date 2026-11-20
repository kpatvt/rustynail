#!/usr/bin/env python3
"""Generates scripts/gate-mutant-map.tsv: which mutants of the trace-compiler registry a changed file can affect.

Sources (docs/GATES.md "Changed files -> mutant groups"):
  * the registry crates/rn-jit/src/tr/mutants.rs (NAMES),
  * the injection sites: every `active("<name>")` in crates/*/src (file, line, enclosing fn),
  * the TABLE of crates/rn-jit/tests/tr_mutants.rs: owner and detecting tests (binary, test name; the file and line that define
    the test are looked up in tests/<bin>.rs and tests/<bin>/*.rs).

Output rows (TAB separated):  kind  path  fn  line  mutant  [owner]
  site  <src file>   <enclosing fn>  <line>  <mutant>   the injection point of the mutant
  det   <test file>  <test fn>       <line>  <mutant>   a test that must catch the mutant (a `*` test = the whole binary: fn `*`)
`scripts/gate-mutant-plan.py` reads it. Run without arguments to rewrite the file, `--check` to verify that the committed file is
current (exit 1 and a diff summary otherwise) and complete (every registered mutant has a site or a detector row)."""
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "scripts", "gate-mutant-map.tsv")
REG = "crates/rn-jit/src/tr/mutants.rs"
TAB = "crates/rn-jit/tests/tr_mutants.rs"


def read(p):
    with open(os.path.join(ROOT, p), encoding="utf-8") as f:
        return f.read()


def registry():
    s = read(REG)
    body = s[s.index("pub const NAMES"):]
    body = body[: body.index("];")]
    return re.findall(r'"([a-z0-9_]+)"', re.sub(r"//[^\n]*", "", body))


def rs_files(d):
    for base, _, files in os.walk(os.path.join(ROOT, d)):
        for f in sorted(files):
            if f.endswith(".rs"):
                yield os.path.relpath(os.path.join(base, f), ROOT)


FN_RE = re.compile(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?(?:unsafe\s+)?(?:const\s+)?(?:extern\s+\"C\"\s+)?fn\s+([A-Za-z0-9_]+)")


def sites(names):
    nameset = set(names)
    out = []
    doc = []
    for crate in sorted(os.listdir(os.path.join(ROOT, "crates"))):
        src = f"crates/{crate}/src"
        if not os.path.isdir(os.path.join(ROOT, src)):
            continue
        for p in rs_files(src):
            if p == REG:
                continue
            fn = "-"
            for i, line in enumerate(read(p).split("\n"), 1):
                m = FN_RE.match(line)
                if m:
                    fn = m.group(1)
                for n in re.findall(r'active\("([a-z0-9_]+)"\)', line):
                    if n in nameset:
                        out.append(("site", p, fn, i, n))
                if line.lstrip().startswith("//"):
                    for n in re.findall(r"`([a-z0-9_]+)`", line):
                        if n in nameset:
                            doc.append(("site", p, fn, i, n))
    # a mutant injected through a setter (`set_mut_x(active("name"))` in mutants.rs) has no `active` call in its crate: the
    # doc comments that name it in backticks mark its site
    have = {r[4] for r in out}
    out += [r for r in doc if r[4] not in have]
    return out


def table():
    """name -> (owner, [(bin, test)])"""
    s = read(TAB)
    s = s[s.index("const TABLE"):]
    s = s[: s.index("\n];")]
    consts = {}
    cs = read(TAB)
    for m in re.finditer(r"const (P7_BINS|[A-Z0-9_]+): &\[Det\] = &\[(.*?)\];", cs, re.S):
        consts[m.group(1)] = re.findall(r'\b(?:d|dl|o|ol)\("([a-z0-9_]+)",\s*"([^"]+)"\)', m.group(2))
    rows = {}
    for m in re.finditer(r'Row \{\s*name: "([a-z0-9_]+)",\s*owner: "([^"]*)",\s*dets: (.*?),?\s*\},\n', s, re.S):
        name, owner, d = m.groups()
        d = d.strip()
        if d.startswith("&["):
            dets = re.findall(r'\b(?:d|dl|o|ol)\("([a-z0-9_]+)",\s*"([^"]+)"\)', d)
        else:
            dets = consts.get(d, [])
        rows[name] = (owner, dets)
    # a Det struct literal (env vars of its own): `Det { bin: "x", test: "y", ...`
    for m in re.finditer(r'Row \{\s*name: "([a-z0-9_]+)",\s*owner: "([^"]*)",\s*dets: &\[(Det \{.*?\})\],\s*\},\n', s, re.S):
        name, owner, d = m.groups()
        rows[name] = (owner, re.findall(r'bin: "([a-z0-9_]+)",\s*test: "([^"]+)"', d))
    return rows


def test_defs():
    """(bin, test) -> (file, line) by looking for `fn <test>` in tests/<bin>.rs and tests/<bin>/*.rs"""
    cache = {}

    def lookup(b, t):
        if (b, t) in cache:
            return cache[(b, t)]
        cands = [f"crates/rn-jit/tests/{b}.rs"]
        d = os.path.join(ROOT, "crates/rn-jit/tests", b)
        if os.path.isdir(d):
            cands += list(rs_files(f"crates/rn-jit/tests/{b}"))
        res = (f"crates/rn-jit/tests/{b}.rs", 0)
        if t != "*":
            for p in cands:
                if not os.path.exists(os.path.join(ROOT, p)):
                    continue
                for i, line in enumerate(read(p).split("\n"), 1):
                    if re.match(r"\s*(?:pub\s+)?fn\s+" + re.escape(t) + r"\b", line):
                        res = (p, i)
                        break
                else:
                    continue
                break
        cache[(b, t)] = res
        return res

    return lookup


def generate():
    names = registry()
    rows = []
    for r in sites(names):
        rows.append(r)
    tb = table()
    look = test_defs()
    for n in names:
        owner, dets = tb.get(n, ("?", []))
        for b, t in dets:
            p, line = look(b, t)
            rows.append(("det", p, t if t != "*" else "*", line, n))
    # registry order of mutants, then site rows before detector rows, stable
    order = {n: i for i, n in enumerate(names)}
    rows = sorted(set(rows), key=lambda r: (order[r[4]], r[0] != "site", r[1], r[3]))
    lines = ["# Generated by scripts/gen-mutant-map.py (check: scripts/gen-mutant-map.py --check). Do not edit.",
             "# kind\tpath\tfn\tline\tmutant"]
    lines += ["\t".join(str(x) for x in r) for r in rows]
    return "\n".join(lines) + "\n", names, rows


def main():
    text, names, rows = generate()
    have = {r[4] for r in rows}
    missing = [n for n in names if n not in have]
    if "--check" in sys.argv:
        bad = 0
        try:
            cur = open(OUT, encoding="utf-8").read()
        except OSError:
            cur = ""
        if cur != text:
            a, b = set(cur.splitlines()), set(text.splitlines())
            print(f"gate-mutant-map.tsv is stale: {len(b - a)} rows missing, {len(a - b)} rows obsolete (run scripts/gen-mutant-map.py)")
            for x in sorted(b - a)[:5]:
                print("  + " + x)
            for x in sorted(a - b)[:5]:
                print("  - " + x)
            bad = 1
        if missing:
            print("registered mutants without a site or a detector row: " + ", ".join(missing))
            bad = 1
        if not bad:
            print(f"gate-mutant-map.tsv ok: {len(names)} mutants, {len(rows)} rows")
        sys.exit(bad)
    with open(OUT, "w", encoding="utf-8") as f:
        f.write(text)
    nsite = len({r[4] for r in rows if r[0] == "site"})
    print(f"wrote {OUT}: {len(names)} mutants, {nsite} with a site, {len(rows)} rows" +
          (f"; no rows for: {', '.join(missing)}" if missing else ""))


if __name__ == "__main__":
    main()
