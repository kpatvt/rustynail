// Port of bench/corpus2/heldout/dijkstra_routes.lua: Dijkstra with a hand-written binary heap, BFS/DFS, path
// reconstruction, all-pairs on a subset of hubs.  Node numbers stay 1-based (index 0 of the arrays is unused) so the
// output matches the Lua program.  The heap keeps its items at h[1..n] (h[0] is a placeholder), as the Lua table does.
const N = Number(process.argv[2]) || 200;

let seed = 777;
function rnd(n) {
  seed = (seed * 16807) % 2147483647;
  return (seed % n) + 1;
}

// ------------------------------------------------------------ binary heap of [dist,node]
function heapNew() { return [null]; }

function heapPush(h, item) {
  let n = h.length;
  h.push(item);
  while (n > 1) {
    const p = Math.floor(n / 2);
    const a = h[n], b = h[p];
    if (a[0] < b[0] || (a[0] === b[0] && a[1] < b[1])) {
      h[n] = b; h[p] = a;
      n = p;
    } else {
      break;
    }
  }
}

function heapPop(h) {
  let n = h.length - 1;
  if (n === 0) return null;
  const top = h[1];
  h[1] = h[n];
  h.pop();
  n = n - 1;
  let i = 1;
  while (true) {
    const l = i * 2, r = i * 2 + 1;
    let s = i;
    if (l <= n && (h[l][0] < h[s][0] || (h[l][0] === h[s][0] && h[l][1] < h[s][1]))) s = l;
    if (r <= n && (h[r][0] < h[s][0] || (h[r][0] === h[s][0] && h[r][1] < h[s][1]))) s = r;
    if (s === i) break;
    const t = h[i]; h[i] = h[s]; h[s] = t;
    i = s;
  }
  return top;
}

// ------------------------------------------------------------ graph construction
const V = 100 + 10 * N;
const xs = [0], ys = [0], adj = [null];
for (let i = 1; i <= V; i++) {
  xs[i] = rnd(1000); ys[i] = rnd(1000);
  adj[i] = [];
}

let edgeCount = 0;
function addRoad(a, b) {
  if (a === b) return;
  for (const e of adj[a]) {
    if (e.to === b) return;
  }
  const dx = xs[a] - xs[b], dy = ys[a] - ys[b];
  const w = Math.floor(Math.sqrt(dx * dx + dy * dy)) + rnd(5);
  adj[a].push({ to: b, w: w });
  adj[b].push({ to: a, w: w });
  edgeCount = edgeCount + 1;
}

// spanning chain keeps the network connected, then local shortcuts and a few highways
for (let i = 2; i <= V; i++) addRoad(i - 1, i);
for (let i = 1; i <= V; i++) {
  for (let t = 0; t < 2; t++) {
    let best = null, bd = 1e18;
    for (let u = 0; u < 6; u++) {
      const j = rnd(V);
      const d = Math.pow(xs[i] - xs[j], 2) + Math.pow(ys[i] - ys[j], 2);
      if (j !== i && d < bd) { best = j; bd = d; }
    }
    if (best !== null) addRoad(i, best);
  }
}
for (let t = 0, m = Math.floor(V / 20); t < m; t++) addRoad(rnd(V), rnd(V));

// ------------------------------------------------------------ algorithms
const INF = 1e15;

function dijkstra(src) {
  const dist = [0], prev = [];
  for (let i = 1; i <= V; i++) dist[i] = INF;
  dist[src] = 0;
  const h = heapNew();
  heapPush(h, [0, src]);
  let settled = 0;
  while (h.length > 1) {
    const top = heapPop(h);
    const d = top[0], u = top[1];
    if (d <= dist[u]) {
      settled = settled + 1;
      const es = adj[u];
      for (let k = 0; k < es.length; k++) {
        const e = es[k];
        const nd = d + e.w;
        if (nd < dist[e.to]) {
          dist[e.to] = nd;
          prev[e.to] = u;
          heapPush(h, [nd, e.to]);
        }
      }
    }
  }
  return { dist, prev, settled };
}

function pathTo(prev, src, dst) {
  const path = [];
  let cur = dst;
  while (cur) {
    path.unshift(cur);
    if (cur === src) return path;
    cur = prev[cur];
  }
  return null;
}

function bfs(src) {
  const hops = [], queue = [src];
  hops[src] = 0;
  let head = 0;
  while (head < queue.length) {
    const u = queue[head];
    head = head + 1;
    for (const e of adj[u]) {
      if (hops[e.to] === undefined) {
        hops[e.to] = hops[u] + 1;
        queue.push(e.to);
      }
    }
  }
  return { hops, count: queue.length };
}

function dfsOrder(src, limit) {
  const seen = [], stack = [src], order = [];
  while (stack.length > 0 && order.length < limit) {
    const u = stack.pop();
    if (!seen[u]) {
      seen[u] = true;
      order.push(u);
      const es = adj[u];
      for (let k = es.length - 1; k >= 0; k--) {
        if (!seen[es[k].to]) stack.push(es[k].to);
      }
    }
  }
  return order;
}

// ------------------------------------------------------------ reports
const degreeHist = new Map();
for (let i = 1; i <= V; i++) {
  const d = adj[i].length;
  degreeHist.set(d, (degreeHist.get(d) || 0) + 1);
}
const degs = [];
for (const d of degreeHist.keys()) degs.push(d);
degs.sort((a, b) => a - b);
const out = [];
out.push(`nodes=${V} roads=${edgeCount}`);
{
  const parts = [];
  for (const d of degs) parts.push(d + ":" + degreeHist.get(d));
  out.push("degrees " + parts.join(" "));
}

let totalSettled = 0, farSum = 0, hopSum = 0, pathSum = 0;
let longest = -1, longestPair = null;
const samples = [];
for (let q = 1; q <= N; q++) {
  const src = rnd(V), dst = rnd(V);
  const { dist, prev, settled } = dijkstra(src);
  totalSettled = totalSettled + settled;
  const p = pathTo(prev, src, dst);
  const hops = bfs(src).hops;
  farSum = farSum + dist[dst];
  hopSum = hopSum + hops[dst];
  pathSum = pathSum + p.length - 1;
  if (dist[dst] > longest) { longest = dist[dst]; longestPair = [src, dst, p.length]; }
  if (q <= 5) {
    const show = [];
    for (let i = 0, m = Math.min(p.length, 6); i < m; i++) show.push(p[i]);
    samples.push(`route ${src}->${dst} dist=${dist[dst]} hops=${p.length - 1} bfs=${hops[dst]} via ${show.join(",")}${p.length > 6 ? ",..." : ""}`);
  }
}
for (const s of samples) out.push(s);
out.push(`queries=${N} settled=${totalSettled} dist_sum=${farSum} bfs_hops=${hopSum} path_hops=${pathSum}`);
out.push(`longest route ${longestPair[0]}->${longestPair[1]} dist=${longest} nodes=${longestPair[2]}`);

// connectivity and DFS
out.push("reachable from 1:\t" + bfs(1).count);
{
  const order = dfsOrder(1, 40);
  out.push("dfs prefix: " + order.slice(0, 15).join(" "));
}

// all pairs on a subset of hubs
const hubs = [0];
for (let i = 1; i <= 12; i++) hubs[i] = rnd(V);
const nh = hubs.length - 1;
const matrix = [null];
let mx = 0, mn = INF, mnPair = null;
for (let i = 1; i <= nh; i++) {
  const dist = dijkstra(hubs[i]).dist;
  matrix[i] = [0];
  for (let j = 1; j <= nh; j++) {
    matrix[i][j] = dist[hubs[j]];
    if (i !== j && dist[hubs[j]] > mx) mx = dist[hubs[j]];
    if (i < j && dist[hubs[j]] < mn) { mn = dist[hubs[j]]; mnPair = [hubs[i], hubs[j]]; }
  }
}
for (let i = 1; i <= 4; i++) {
  const row = [];
  for (let j = 1; j <= 8; j++) row.push(String(matrix[i][j]).padStart(5, " "));
  out.push(row.join(" "));
}
out.push(`hub diameter=${mx} closest pair ${mnPair[0]}-${mnPair[1]} dist=${mn}`);

// triangle inequality spot-check over the matrix
let violations = 0;
for (let i = 1; i <= nh; i++) {
  for (let j = 1; j <= nh; j++) {
    for (let k = 1; k <= nh; k++) {
      if (matrix[i][j] > matrix[i][k] + matrix[k][j]) violations = violations + 1;
    }
  }
}
out.push("triangle violations:\t" + violations);

// centrality-ish summary: how often each hub lies on a route between other hubs
const counts = new Map();
for (let i = 1; i <= nh; i++) {
  const prev = dijkstra(hubs[i]).prev;
  for (let j = 1; j <= nh; j++) {
    const p = pathTo(prev, hubs[i], hubs[j]);
    for (const node of p) counts.set(node, (counts.get(node) || 0) + 1);
  }
}
const nodes = [];
for (const node of counts.keys()) nodes.push(node);
nodes.sort((a, b) => {
  if (counts.get(a) !== counts.get(b)) return counts.get(b) - counts.get(a);
  return a - b;
});
for (let i = 0; i < 5; i++) out.push(`busy node ${nodes[i]} used ${counts.get(nodes[i])} times`);
console.log(out.join("\n"));
