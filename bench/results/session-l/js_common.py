import math, re
def load(p):
    out, cur = {}, None
    for l in open(p):
        if l.startswith('## Default'): cur = 'def'; out[cur] = {}
        elif l.startswith('## 20x'): cur = '20x'; out[cur] = {}
        elif cur and l.startswith('| ') and not l.startswith('| program') and not l.startswith('|---'):
            c = [x.strip() for x in l.strip().strip('|').split('|')]
            out[cur][c[0]] = (float(c[4]), None if c[7] == 'n/a' else float(c[7]))
    return out
j = load('../session-j/js_vs_node.md'); k = load('js_vs_node.md')
gm = lambda xs: math.exp(sum(math.log(x) for x in xs) / len(xs))
for s in ('def', '20x'):
    com = [n for n in k[s] if n in j[s]]
    print(s, 'common', len(com), 'ours/node session-j %.3f now %.3f' % (gm([j[s][n][0] for n in com]), gm([k[s][n][0] for n in com])))
    cl = [n for n in com if j[s][n][1] and k[s][n][1]]
    print(s, 'ours/LJ  common', len(cl), 'session-j %.3f now %.3f' % (gm([j[s][n][1] for n in cl]), gm([k[s][n][1] for n in cl])))
    for n in com:
        if abs(k[s][n][0] / j[s][n][0] - 1) > 0.1:
            print('  ', n, j[s][n][0], '->', k[s][n][0])
