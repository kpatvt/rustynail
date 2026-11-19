const N = Number(process.argv[2]) || 6;
const BB_TOP = 0, BB_NONHEADER = 1, BB_REDUCIBLE = 2, BB_SELF = 3, BB_IRREDUCIBLE = 4, BB_DEAD = 5;
const MAXNONBACKPREDS = 32 * 1024;

class BasicBlock {
  constructor(name) { this.name = name; this.inEdges = []; this.outEdges = []; }
}

class CFG {
  constructor() { this.nodes = []; this.n = 0; this.edges = []; this.start = null; }
  createNode(name) {
    let node = this.nodes[name];
    if (node === undefined) {
      node = new BasicBlock(name);
      this.nodes[name] = node;
      this.n = this.n + 1;
      if (this.start === null) this.start = node;
    }
    return node;
  }
  addEdge(from, to) {
    this.edges.push([from, to]);
    from.outEdges.push(to);
    to.inEdges.push(from);
  }
}

class SimpleLoop {
  constructor(bb, reducible) {
    this.header = bb; this.reducible = reducible; this.parent = null; this.children = []; this.blocks = [];
    this.nesting = 0; this.depth = 0; this.counter = 0; this.isRoot = false;
  }
}

function calcNesting(loop, depth) {
  loop.depth = depth;
  for (let i = 0; i < loop.children.length; i++) calcNesting(loop.children[i], depth + 1);
  if (loop.parent !== null) {
    const n = loop.nesting;
    if (loop.parent.nesting < n + 1) loop.parent.nesting = n + 1;
  }
}

class LSG {
  constructor() {
    const root = new SimpleLoop(null, false);
    root.isRoot = true;
    this.root = root; this.loops = [root]; this.counter = 1;
    root.counter = 0;
  }
  createNewLoop(bb, reducible) {
    const loop = new SimpleLoop(bb, reducible);
    loop.counter = this.counter;
    this.counter = this.counter + 1;
    this.loops.push(loop);
    return loop;
  }
  calculateNestingLevel() {
    for (let i = 0; i < this.loops.length; i++) {
      const loop = this.loops[i];
      if (!loop.isRoot) {
        if (loop.parent === null) loop.parent = this.root;
        loop.parent.children.push(loop);
      }
    }
    calcNesting(this.root, 0);
  }
}

class UF {
  constructor() { this.parent = null; this.bb = null; this.loop = null; this.dfs = 0; }
  init(bb, dfs) {
    this.parent = this;
    this.bb = bb;
    this.dfs = dfs;
  }
  findSet() {
    const list = [];
    let node = this;
    while (node !== node.parent) {
      if (node.parent !== node.parent.parent) list.push(node);
      node = node.parent;
    }
    for (let i = 0; i < list.length; i++) list[i].parent = node.parent;
    return node;
  }
  union(other) { this.parent = other; }
}

function isAncestor(w, v, last) { return w <= v && v <= last[w]; }

function dfs(cur, nodes, number, last, current) {
  nodes[current].init(cur, current);
  number.set(cur, current);
  let lastid = current;
  const outs = cur.outEdges;
  for (let i = 0; i < outs.length; i++) {
    const target = outs[i];
    if (!number.has(target)) {
      lastid = dfs(target, nodes, number, last, lastid + 1);
    }
  }
  last[number.get(cur)] = lastid;
  return lastid;
}

function findLoops(cfg, lsg) {
  if (cfg.start === null) return;
  const size = cfg.n;
  const nonBackPreds = [], backPreds = [], header = [], type_ = [], last = [], nodes = [];
  const number = new Map();
  for (let i = 0; i < size; i++) {
    nonBackPreds[i] = [];
    backPreds[i] = [];
    nodes[i] = new UF();
  }
  dfs(cfg.start, nodes, number, last, 0);
  for (let w = 0; w < size; w++) {
    header[w] = 0;
    type_[w] = BB_NONHEADER;
    const nodeW = nodes[w].bb;
    if (nodeW === null) {
      type_[w] = BB_DEAD;
    } else {
      const ins = nodeW.inEdges;
      for (let i = 0; i < ins.length; i++) {
        const v = number.get(ins[i]);
        if (v !== undefined) {
          if (isAncestor(w, v, last)) backPreds[w].push(v); else nonBackPreds[w].push(v);
        }
      }
    }
  }
  header[0] = 0;
  for (let w = size - 1; w >= 0; w--) {
    const nodePool = [];
    const inPool = new Set();
    const nodeW = nodes[w].bb;
    if (nodeW !== null) {
      const bp = backPreds[w];
      for (let i = 0; i < bp.length; i++) {
        const v = bp[i];
        if (v !== w) {
          const s = nodes[v].findSet();
          if (!inPool.has(s)) { inPool.add(s); nodePool.push(s); }
        } else {
          type_[w] = BB_SELF;
        }
      }
      const worklist = [];
      for (let i = 0; i < nodePool.length; i++) worklist[i] = nodePool[i];
      if (nodePool.length !== 0) type_[w] = BB_REDUCIBLE;
      let wi = 0;
      while (wi < worklist.length) {
        const x = worklist[wi];
        wi = wi + 1;
        const nbp = nonBackPreds[x.dfs];
        if (nbp.length > MAXNONBACKPREDS) return;
        for (let i = 0; i < nbp.length; i++) {
          const y = nodes[nbp[i]];
          const ydash = y.findSet();
          if (!isAncestor(w, ydash.dfs, last)) {
            type_[w] = BB_IRREDUCIBLE;
            nonBackPreds[w].push(ydash.dfs);
          } else {
            if (ydash.dfs !== w && !inPool.has(ydash)) {
              worklist.push(ydash);
              nodePool.push(ydash);
              inPool.add(ydash);
            }
          }
        }
      }
      if (nodePool.length > 0 || type_[w] === BB_SELF) {
        const loop = lsg.createNewLoop(nodeW, type_[w] !== BB_IRREDUCIBLE);
        nodes[w].loop = loop;
        for (let i = 0; i < nodePool.length; i++) {
          const node = nodePool[i];
          header[node.dfs] = w;
          node.union(nodes[w]);
          if (node.loop !== null) {
            node.loop.parent = loop;
          } else {
            loop.blocks.push(node.bb);
          }
        }
      }
    }
  }
}

let cfg, lsg;
function buildDiamond(start) {
  const b0 = cfg.createNode(start);
  const b1 = cfg.createNode(start + 1);
  const b2 = cfg.createNode(start + 2);
  const b3 = cfg.createNode(start + 3);
  cfg.addEdge(b0, b1);
  cfg.addEdge(b0, b2);
  cfg.addEdge(b1, b3);
  cfg.addEdge(b2, b3);
  return start + 3;
}
function buildConnect(s, e) { cfg.addEdge(cfg.createNode(s), cfg.createNode(e)); }
function buildStraight(start, n) {
  for (let i = 0; i <= n - 1; i++) buildConnect(start + i, start + i + 1);
  return start + n;
}
function buildBaseLoop(from) {
  const header = buildStraight(from, 1);
  const diamond1 = buildDiamond(header);
  const d11 = buildStraight(diamond1, 1);
  const diamond2 = buildDiamond(d11);
  const footer = buildStraight(diamond2, 1);
  buildConnect(diamond2, d11);
  buildConnect(diamond1, header);
  buildConnect(footer, from);
  return buildStraight(footer, 1);
}

function buildCFG(parLoops, pparLoops, ppparLoops) {
  cfg = new CFG();
  lsg = new LSG();
  cfg.createNode(0);
  buildBaseLoop(0);
  cfg.createNode(1);
  buildConnect(0, 2);
  for (let dummy = 1; dummy <= 20; dummy++) findLoops(cfg, new LSG());
  let n = 2;
  for (let p = 1; p <= parLoops; p++) {
    cfg.createNode(n + 1);
    buildConnect(2, n + 1);
    n = n + 1;
    for (let i = 1; i <= pparLoops; i++) {
      const top = n;
      n = buildStraight(n, 1);
      for (let j = 1; j <= ppparLoops; j++) n = buildBaseLoop(n);
      const bottom = buildStraight(n, 1);
      buildConnect(n, top);
      n = bottom;
    }
    buildConnect(n, 1);
  }
}

buildCFG(10, 10, 5);
let loops = 0, levels = 0;
for (let it = 1; it <= N; it++) {
  const g = new LSG();
  findLoops(cfg, g);
  g.calculateNestingLevel();
  loops = g.loops.length;
  levels = 0;
  for (let i = 0; i < g.loops.length; i++) levels = levels + g.loops[i].nesting + g.loops[i].depth;
}
console.log(`havlak\t${loops}\t${levels}\t${cfg.n}`);
