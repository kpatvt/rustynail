#!/usr/bin/env python3
"""Generates NOTICE (repo root): project/author/licence pointers, the Lua.org attribution and the licences of the
third-party Rust crates that are linked into the release binaries (rnlua, rnluac).

    scripts/gen-notice.py            # rewrite NOTICE
    scripts/gen-notice.py --check    # exit 1 if NOTICE is out of date (CI / release-build.sh)

Source of truth: `cargo metadata --offline` for the `rn-cli` package, normal (non-dev, non-build) dependency edges
only, for the x86_64-unknown-linux-gnu target (the JIT target; the other targets pull in no further crates). Crates
from the workspace itself (path sources) are excluded. Build-only tools (the `cc` crate that compiles the C shim of
rn-capi) are not linked into the binaries and are therefore not listed. For every dependency the licence expression of
its manifest is printed together with the text of its licence files found in the crate's source directory (from the
cargo registry; run `cargo fetch` first when building from scratch). A dual-licensed crate (`A OR B`) is used under
the first alternative of which a licence file is present, preferring MIT.
"""
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = "x86_64-unknown-linux-gnu"
ROOT_PKG = "rn-cli"

HEADER = """\
rustynail
=========

rnlua, rnluac (and the rustynail libraries): a Lua 5.5 (and 5.4) implementation in Rust with a template JIT and a trace
compiler.

Copyright (c) 2026 Kiran Pashikanti <kpatvt@gmail.com>

rustynail is licensed under the MIT license (SPDX-License-Identifier: MIT); the full text is in the file LICENSE and is
printed by `rnlua --license`.


Lua
---

rustynail is a derivative port of the Lua interpreter, versions 5.5.1 and 5.4.9:

    Lua 5.5.1 and 5.4.9  Copyright (C) 1994-2026 Lua.org, PUC-Rio
    https://www.lua.org/

Lua is licensed under the MIT license; the notice that must accompany copies of rustynail is in the file LICENSE-LUA
and is printed by `rnlua --license` after the rustynail licence. The port reproduces Lua's behaviour (error messages,
formats, bytecode format) and is compatible with Lua's C API headers.


Third-party Rust crates linked into the binaries
------------------------------------------------

The crates below are compiled into rnlua and rnluac (normal dependencies of the `rn-cli` package, x86-64 Linux).
Regenerate this section with scripts/gen-notice.py.
"""


def metadata():
    out = subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--offline", "--filter-platform", TARGET],
        cwd=ROOT, text=True)
    return json.loads(out)


def normal_closure(md):
    pkgs = {p["id"]: p for p in md["packages"]}
    nodes = {n["id"]: n for n in md["resolve"]["nodes"]}
    root = next(p["id"] for p in md["packages"] if p["name"] == ROOT_PKG and p["source"] is None)
    seen, stack = set(), [root]
    while stack:
        i = stack.pop()
        if i in seen:
            continue
        seen.add(i)
        for d in nodes[i]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):  # None = normal edge
                stack.append(d["pkg"])
    return sorted((pkgs[i] for i in seen if pkgs[i]["source"] is not None), key=lambda p: (p["name"], p["version"]))


LICENSE_FILE = re.compile(r"^(LICENSE|LICENCE|COPYING|UNLICENSE)([-_.].*)?$", re.I)


def license_files(pkgdir):
    return sorted(f for f in os.listdir(pkgdir) if LICENSE_FILE.match(f) and os.path.isfile(os.path.join(pkgdir, f)))


def pick(expr, files):
    """The licence files to print for an SPDX expression: all files for a conjunction, the preferred alternative for OR."""
    alts = [a.strip(" ()") for a in re.split(r"\s+OR\s+", expr)]
    if len(alts) == 1:
        return files, expr
    for pref in ["MIT"] + alts:
        key = pref.upper().replace("-", "")
        hit = [f for f in files if key.split("-")[0] in f.upper().replace("-", "")]
        if hit and pref in alts:
            return hit, pref
    return files, expr


def render(md):
    out = [HEADER]
    for p in normal_closure(md):
        pkgdir = os.path.dirname(p["manifest_path"])
        expr = p.get("license") or "UNKNOWN"
        files = license_files(pkgdir)
        chosen, used = pick(expr, files)
        out.append("")
        out.append("%s %s" % (p["name"], p["version"]))
        out.append("-" * len("%s %s" % (p["name"], p["version"])))
        out.append("License: %s%s" % (expr, "" if used == expr else "  (used under %s)" % used))
        if p.get("repository"):
            out.append("Source: %s" % p["repository"])
        if not chosen:
            out.append("(no licence file shipped in the crate; see the manifest licence field above)")
        for f in chosen:
            text = open(os.path.join(pkgdir, f), encoding="utf-8", errors="replace").read().strip("\n")
            out.append("")
            out.append("[%s]" % f)
            out.append("")
            out.append(text)
    return "\n".join(out).rstrip("\n") + "\n"


def main():
    new = render(metadata())
    path = os.path.join(ROOT, "NOTICE")
    if "--check" in sys.argv:
        old = open(path, encoding="utf-8").read() if os.path.exists(path) else ""
        if old != new:
            sys.stderr.write("NOTICE is out of date: run scripts/gen-notice.py\n")
            sys.exit(1)
        return
    with open(path, "w", encoding="utf-8") as f:
        f.write(new)
    print("wrote %s (%d bytes)" % (path, len(new)))


if __name__ == "__main__":
    main()
