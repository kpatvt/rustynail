import csv
rows = {r['program']: r for r in csv.DictReader(open('per_program.csv'))}
f = lambda r, c, k: float(r[f"{c}_{k}_min"])
for k in ['jit-trace/array_rec', 'jit-trace/int_rec2', 'jit-trace/rec_loop', 'jit-trace/int_sum', 'awfy/queens', 'programs/queens', 'awfy/towers', 'awfy/list', 'awfy/cd', 'apps/raytrace', 'awfy/richards', 'awfy/deltablue', 'programs/fib', 'programs/binary_trees', 'awfy/permute']:
    r = rows[k]
    print(k, ' | '.join(f"{c} {f(r,c,'cyc')/f(r,'b_'+c,'cyc'):.3f}/{f(r,c,'ins')/f(r,'b_'+c,'ins'):.3f}" for c in ('plain', 'pgo', 'p54', 'g54')), 'plain %.0fM' % (f(r, 'plain', 'cyc') / 1e6))
