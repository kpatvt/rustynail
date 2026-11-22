import json
A = json.load(open('fast_all.json'))['results']
B = json.load(open('b116_fast_all.json'))['results']
FOC = {"programs/nbody", "programs/fib", "programs/binary_trees", "programs/queens", "apps/raytrace", "awfy/cd", "awfy/list", "awfy/richards", "awfy/deltablue", "awfy/havlak", "awfy/permute", "programs/permute", "awfy/towers", "corpus/lua_lexer", "corpus/classic_vector", "corpus2/heldout/dijkstra_routes", "programs/sumrec", "jit-trace/array_rec", "jit-trace/int_rec2", "jit-trace/rec_loop", "awfy/queens"}
out = {}
for k, e in A.items():
    why = []
    for c in ('plain', 'pgo', 'p54', 'g54'):
        x = [v for v in e['cols'][c]['cyc'] if v]
        y = [v for v in B[k]['cols'][c]['cyc'] if v]
        if not x or not y:
            continue
        r = min(x) / min(y)
        if min(x) >= 20e6:
            if r > 1.03:
                why.append(f"{c}:reg{r:.2f}")
            if r < 0.93:
                why.append(f"{c}:imp{r:.2f}")
            if max(x) / min(x) - 1 > 0.05:
                why.append(f"{c}:noisy{max(x)/min(x)-1:.2f}")
    if k in FOC and not why:
        why.append("focus")
    if why:
        out[k] = why
json.dump(out, open('rerun_why.json', 'w'), indent=0)
open('rerun_programs_all.txt', 'w').write("\n".join(sorted(out)) + "\n")
print(len(out))
