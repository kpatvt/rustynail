import json,sys
d=json.load(open(sys.argv[1]))
for r in d["results"].values():
    c=r["cols"]
    for a,b in (("pplain","qplain"),("ppgo","qpgo")):
        if a in c and b in c: c[a],c[b]=c[b],c[a]
json.dump(d,open(sys.argv[2],"w"))
