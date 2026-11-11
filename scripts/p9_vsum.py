import re, sys
# per program: trace summary, aborts, code bytes of installed traces; tier-1 code bytes
txt = open(sys.argv[1]).read()
blocks = re.split(r"^##### ", txt, flags=re.M)[1:]
rows = {}
for b in blocks:
    head, _, body = b.partition("\n")
    name = head.split()[0]
    if "TRACE=on" in head:
        r = rows.setdefault(name, {})
        m = re.search(r"\[trace\] recorded (\d+) installed (\d+) entered (\d+) flushes (\d+) rerecord (\d+) blacklisted (\d+)", body)
        r["sum"] = m.groups() if m else None
        r["code"] = sum(int(x) for x in re.findall(r"loop proto:\S+ pc \d+ entry 0x[0-9a-f]+ code (\d+)B", body))
        r["tr_ntraces"] = len(re.findall(r" loop proto:", body))
        r["aborts"] = re.findall(r"\[trace\] aborted (\S+): (\d+)", body)
        m = re.search(r"\[trace\] exits (\d+)", body)
        r["exits"] = m.group(1) if m else "?"
        m = re.search(r"hot exits: type-guard (\d+) other (\d+)", body)
        r["hot"] = m.groups() if m else None
        m = re.search(r"record\+compile ns (\d+)", body)
        r["recns"] = int(m.group(1)) if m else None
    else:
        m = re.search(r"code (\d+) B", body)
        rows.setdefault(name, {})["t1code"] = int(m.group(1)) if m else None
for n in sorted(rows):
    r = rows[n]
    t1 = r.get("t1code")
    ratio = f"{r['code'] / t1:.2f}" if t1 and r["code"] else "-"
    print(f"{n:<18} rec/inst/entered/flush/rerec/black={r['sum']} exits={r['exits']} hot(tg,other)={r['hot']} traces_code={r['code']}B t1_code={t1}B ratio={ratio}")
    print(f"{'':<18} aborts: {r['aborts']}")
