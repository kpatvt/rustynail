import re,sys,glob
pat=re.compile(r'(\w+)\[(\w+)\]\s*=\s*[^=\n]*\b\1\[\2\]')
pat2=re.compile(r'(\w+)\[(\w+)\]\s*=\s*\(?\1\[\2\]')
for d in ['bench/programs','bench/real','bench/jit-trace','bench/lj/programs','bench/lj/real','bench/lj/jit-trace','bench/lib']:
    for f in sorted(glob.glob(d+'/*.lua')):
        hits=[l.strip() for l in open(f) if pat.search(l) and not re.search(r'\[\s*\d+\s*\]',l.split('=')[0])]
        # skip numeric-index patterns like a[i] = a[i]+... (i numeric): keep all, filter later by hand
        if hits: print(f, len(hits), hits[0][:90])
