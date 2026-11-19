const N = Number(process.argv[2]) || 1;
const NUM_AIRCRAFT = 100;
const NUM_FRAMES = 100;
const PROXIMITY_RADIUS = 1.0;
const VOXEL_SIZE = 10.0;
const { sqrt, cos, sin, floor } = Math;

class Vector2D {
  constructor(x, y) { this.x = x; this.y = y; }
  compareTo(o) {
    if (this.x < o.x) return -1;
    if (this.x > o.x) return 1;
    if (this.y < o.y) return -1;
    if (this.y > o.y) return 1;
    return 0;
  }
  plus(o) { return new Vector2D(this.x + o.x, this.y + o.y); }
}
class Vector3D {
  constructor(x, y, z) { this.x = x; this.y = y; this.z = z; }
  plus(o) { return new Vector3D(this.x + o.x, this.y + o.y, this.z + o.z); }
  minus(o) { return new Vector3D(this.x - o.x, this.y - o.y, this.z - o.z); }
  dot(o) { return this.x * o.x + this.y * o.y + this.z * o.z; }
  times(a) { return new Vector3D(this.x * a, this.y * a, this.z * a); }
  squaredMagnitude() { return this.dot(this); }
}
class CallSign {
  constructor(v) { this.value = v; }
  compareTo(o) {
    if (this.value < o.value) return -1;
    if (this.value > o.value) return 1;
    return 0;
  }
}

// Red-black tree (left-leaning), keys have compareTo
function isRed(h) { return h !== null && h.red; }
function rotateLeft(h) {
  const x = h.right;
  h.right = x.left;
  x.left = h;
  x.red = h.red;
  h.red = true;
  return x;
}
function rotateRight(h) {
  const x = h.left;
  h.left = x.right;
  x.right = h;
  x.red = h.red;
  h.red = true;
  return x;
}
function flipColors(h) {
  h.red = !h.red;
  h.left.red = !h.left.red;
  h.right.red = !h.right.red;
}
function balance(h) {
  if (isRed(h.right)) h = rotateLeft(h);
  if (isRed(h.left) && isRed(h.left.left)) h = rotateRight(h);
  if (isRed(h.left) && isRed(h.right)) flipColors(h);
  return h;
}
function moveRedLeft(h) {
  flipColors(h);
  if (isRed(h.right.left)) {
    h.right = rotateRight(h.right);
    h = rotateLeft(h);
    flipColors(h);
  }
  return h;
}
function moveRedRight(h) {
  flipColors(h);
  if (isRed(h.left.left)) {
    h = rotateRight(h);
    flipColors(h);
  }
  return h;
}
let oldValue;
function put(tree, h, key, val) {
  if (h === null) {
    tree.size = tree.size + 1;
    oldValue = null;
    return { key: key, value: val, red: true, left: null, right: null };
  }
  const cmp = key.compareTo(h.key);
  if (cmp < 0) {
    h.left = put(tree, h.left, key, val);
  } else if (cmp > 0) {
    h.right = put(tree, h.right, key, val);
  } else {
    oldValue = h.value;
    h.value = val;
  }
  if (isRed(h.right) && !isRed(h.left)) h = rotateLeft(h);
  if (isRed(h.left) && isRed(h.left.left)) h = rotateRight(h);
  if (isRed(h.left) && isRed(h.right)) flipColors(h);
  return h;
}
function minNode(h) {
  while (h.left !== null) h = h.left;
  return h;
}
function deleteMin(h) {
  if (h.left === null) return null;
  if (!isRed(h.left) && !isRed(h.left.left)) h = moveRedLeft(h);
  h.left = deleteMin(h.left);
  return balance(h);
}
function del(h, key) {
  if (key.compareTo(h.key) < 0) {
    if (!isRed(h.left) && !isRed(h.left.left)) h = moveRedLeft(h);
    h.left = del(h.left, key);
  } else {
    if (isRed(h.left)) h = rotateRight(h);
    if (key.compareTo(h.key) === 0 && h.right === null) return null;
    if (!isRed(h.right) && !isRed(h.right.left)) h = moveRedRight(h);
    if (key.compareTo(h.key) === 0) {
      const x = minNode(h.right);
      h.key = x.key;
      h.value = x.value;
      h.right = deleteMin(h.right);
    } else {
      h.right = del(h.right, key);
    }
  }
  return balance(h);
}
function forEachNode(h, f) {
  if (h === null) return;
  forEachNode(h.left, f);
  f(h.key, h.value);
  forEachNode(h.right, f);
}
class RBT {
  constructor() { this.root = null; this.size = 0; }
  put(key, val) {
    this.root = put(this, this.root, key, val);
    this.root.red = false;
    return oldValue;
  }
  get(key) {
    let x = this.root;
    while (x !== null) {
      const cmp = key.compareTo(x.key);
      if (cmp < 0) x = x.left; else if (cmp > 0) x = x.right; else return x.value;
    }
    return null;
  }
  remove(key) {
    if (this.get(key) === null) return null;
    const v = this.get(key);
    if (!isRed(this.root.left) && !isRed(this.root.right)) this.root.red = true;
    this.root = del(this.root, key);
    if (this.root !== null) this.root.red = false;
    this.size = this.size - 1;
    return v;
  }
  forEach(f) { forEachNode(this.root, f); }
}

class Motion {
  constructor(callsign, posOne, posTwo) { this.callsign = callsign; this.posOne = posOne; this.posTwo = posTwo; }
  delta() { return this.posTwo.minus(this.posOne); }
  // closest approach of two spheres moving linearly during t in [0,1]; returns the collision point or null
  findIntersection(other) {
    const v = this.posOne.minus(other.posOne);
    const w = this.delta().minus(other.delta());
    const a = w.dot(w);
    const radius2 = (2 * PROXIMITY_RADIUS) * (2 * PROXIMITY_RADIUS);
    let t;
    if (a !== 0.0) {
      t = -v.dot(w) / a;
      if (t < 0.0) t = 0.0; else if (t > 1.0) t = 1.0;
    } else {
      t = 0.0;
    }
    const d = v.plus(w.times(t));
    if (d.squaredMagnitude() <= radius2) {
      return this.posOne.plus(this.delta().times(t));
    }
    return null;
  }
}

function voxelHash(p) {
  const xDiv = floor(p.x / VOXEL_SIZE);
  const yDiv = floor(p.y / VOXEL_SIZE);
  return new Vector2D(VOXEL_SIZE * xDiv, VOXEL_SIZE * yDiv);
}

function isInVoxel(voxel, motion) {
  const init = motion.posOne, fin = motion.posTwo;
  const xLo = voxel.x - PROXIMITY_RADIUS, xHi = voxel.x + VOXEL_SIZE + PROXIMITY_RADIUS,
    yLo = voxel.y - PROXIMITY_RADIUS, yHi = voxel.y + VOXEL_SIZE + PROXIMITY_RADIUS;
  let t0 = 0.0, t1 = 1.0;
  const dx = fin.x - init.x, dy = fin.y - init.y;
  if (dx === 0.0) {
    if (init.x < xLo || init.x > xHi) return false;
  } else {
    let a = (xLo - init.x) / dx, b = (xHi - init.x) / dx;
    if (a > b) { const tmp = a; a = b; b = tmp; }
    if (a > t0) t0 = a;
    if (b < t1) t1 = b;
  }
  if (dy === 0.0) {
    if (init.y < yLo || init.y > yHi) return false;
  } else {
    let a = (yLo - init.y) / dy, b = (yHi - init.y) / dy;
    if (a > b) { const tmp = a; a = b; b = tmp; }
    if (a > t0) t0 = a;
    if (b < t1) t1 = b;
  }
  return t0 <= t1;
}

const DIRS = [new Vector2D(-VOXEL_SIZE, -VOXEL_SIZE), new Vector2D(-VOXEL_SIZE, 0), new Vector2D(-VOXEL_SIZE, VOXEL_SIZE),
  new Vector2D(0, -VOXEL_SIZE), new Vector2D(0, VOXEL_SIZE), new Vector2D(VOXEL_SIZE, -VOXEL_SIZE), new Vector2D(VOXEL_SIZE, 0),
  new Vector2D(VOXEL_SIZE, VOXEL_SIZE)];

function recurse(voxelMap, seen, nextVoxel, motion) {
  if (!isInVoxel(nextVoxel, motion)) return;
  if (seen.put(nextVoxel, true) !== null) return;
  let list = voxelMap.get(nextVoxel);
  if (list === null) {
    list = [];
    voxelMap.put(nextVoxel, list);
  }
  list.push(motion);
  for (let i = 0; i < 8; i++) recurse(voxelMap, seen, nextVoxel.plus(DIRS[i]), motion);
}

function reduceCollisionSet(motions) {
  const voxelMap = new RBT();
  for (let i = 0; i < motions.length; i++) {
    const m = motions[i];
    recurse(voxelMap, new RBT(), voxelHash(m.posOne), m);
  }
  const result = [];
  voxelMap.forEach(function (k, list) { if (list.length > 1) result.push(list); });
  return result;
}

let state = new RBT();
function handleNewFrame(frame) {
  const motions = [];
  const seen = new RBT();
  for (let i = 0; i < frame.length; i++) {
    const f = frame[i];
    let oldPos = state.put(f.callsign, f.pos);
    if (oldPos === null) oldPos = f.pos;
    motions.push(new Motion(f.callsign, oldPos, f.pos));
    seen.put(f.callsign, true);
  }
  const toRemove = [];
  state.forEach(function (k, v) { if (seen.get(k) === null) toRemove.push(k); });
  for (let i = 0; i < toRemove.length; i++) state.remove(toRemove[i]);
  const reduced = reduceCollisionSet(motions);
  let collisions = 0;
  for (let r = 0; r < reduced.length; r++) {
    const list = reduced[r];
    for (let i = 0; i < list.length; i++) {
      for (let j = i + 1; j < list.length; j++) {
        if (list[i].findIntersection(list[j]) !== null) collisions = collisions + 1;
      }
    }
  }
  return [collisions, reduced.length];
}

function simulate(time, f) {
  const frame = [];
  const n = (f % 10 === 5) ? 90 : NUM_AIRCRAFT;  // some aircraft leave for one frame (exercises tree removal)
  for (let i = 1; i <= n; i += 2) {
    frame.push({ callsign: new CallSign(i), pos: new Vector3D(time, cos(time) * 2 + i * 3, 10) });
    frame.push({ callsign: new CallSign(i + 1), pos: new Vector3D(time, sin(time) * 2 + i * 3, 10) });
  }
  return frame;
}

let collisions = 0, groups = 0;
for (let it = 1; it <= N; it++) {
  state = new RBT();
  for (let f = 0; f <= NUM_FRAMES - 1; f++) {
    const [c, g] = handleNewFrame(simulate(f * 0.1, f));
    collisions = collisions + c;
    groups = groups + g;
  }
}
console.log(`cd\t${collisions}\t${groups}`);
