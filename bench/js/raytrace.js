const ROUNDS = Number(process.argv[2]) || 2;
const W = 96, H = 72;
const { sqrt, floor, max, min } = Math;

class V {
  constructor(x, y, z) { this.x = x; this.y = y; this.z = z; }
  add(o) { return new V(this.x + o.x, this.y + o.y, this.z + o.z); }
  sub(o) { return new V(this.x - o.x, this.y - o.y, this.z - o.z); }
  mul(k) { return new V(this.x * k, this.y * k, this.z * k); }
  dot(o) { return this.x * o.x + this.y * o.y + this.z * o.z; }
  norm() {
    const l = sqrt(this.dot(this));
    return new V(this.x / l, this.y / l, this.z / l);
  }
}
const vec = (x, y, z) => new V(x, y, z);

class Sphere {
  constructor(c, r, col, refl) { this.c = c; this.r = r; this.col = col; this.refl = refl; }
  hit(o, d) {
    const oc = o.sub(this.c);
    const b = oc.dot(d);
    const c = oc.dot(oc) - this.r * this.r;
    const disc = b * b - c;
    if (disc < 0) return null;
    const s = sqrt(disc);
    let t = -b - s;
    if (t > 1e-4) return t;
    t = -b + s;
    if (t > 1e-4) return t;
    return null;
  }
  normal(p) { return p.sub(this.c).mul(1 / this.r); }
  color(p) { return this.col; }
}

class Plane {
  constructor(y, refl) { this.y = y; this.refl = refl; }
  hit(o, d) {
    if (d.y > -1e-6 && d.y < 1e-6) return null;
    const t = (this.y - o.y) / d.y;
    if (t > 1e-4) return t;
    return null;
  }
  normal(p) { return vec(0, 1, 0); }
  color(p) {
    const cx = floor(p.x), cz = floor(p.z);
    if ((((cx + cz) % 2) + 2) % 2 === 0) return vec(0.9, 0.9, 0.9);
    return vec(0.2, 0.2, 0.25);
  }
}

const objs = [
  new Sphere(vec(0, 1, 6), 1, vec(0.9, 0.2, 0.2), 0.3),
  new Sphere(vec(-2.2, 0.7, 5), 0.7, vec(0.2, 0.9, 0.3), 0.2),
  new Sphere(vec(2, 0.5, 4.5), 0.5, vec(0.2, 0.3, 0.95), 0.5),
  new Plane(0, 0.25),
];
const lights = [vec(-5, 8, -2), vec(6, 5, 0)];
let rays = 0;

function trace(o, d, depth) {
  rays = rays + 1;
  let best = null, bestT = 1e30;
  for (let i = 0; i < objs.length; i++) {
    const t = objs[i].hit(o, d);
    if (t !== null && t < bestT) { best = objs[i]; bestT = t; }
  }
  if (best === null) {
    const k = 0.5 * (d.y + 1);
    return vec(0.6 + 0.2 * k, 0.7 + 0.2 * k, 1.0);
  }
  const p = o.add(d.mul(bestT));
  const n = best.normal(p);
  const base = best.color(p);
  let shade = 0.15;
  for (let li = 0; li < lights.length; li++) {
    let ldir = lights[li].sub(p);
    const dist = sqrt(ldir.dot(ldir));
    ldir = ldir.mul(1 / dist);
    let lit = true;
    for (let i = 0; i < objs.length; i++) {
      const t = objs[i].hit(p, ldir);
      if (t !== null && t < dist) { lit = false; break; }
    }
    if (lit) shade = shade + 0.6 * max(0, n.dot(ldir)) / lights.length;
  }
  let col = base.mul(shade);
  if (depth < 3 && best.refl > 0) {
    const r = d.sub(n.mul(2 * d.dot(n)));
    col = col.mul(1 - best.refl).add(trace(p, r, depth + 1).mul(best.refl));
  }
  return col;
}

function render() {
  let sum = 0;
  const eye = vec(0, 1.5, -3);
  for (let y = 0; y <= H - 1; y++) {
    for (let x = 0; x <= W - 1; x++) {
      const dx = (x - W / 2) / H * 1.4;
      const dy = (H / 2 - y) / H * 1.4 - 0.1;
      const d = vec(dx, dy, 1).norm();
      const c = trace(eye, d, 0);
      sum = sum + floor(min(1, c.x) * 255) + floor(min(1, c.y) * 255) + floor(min(1, c.z) * 255);
    }
  }
  return sum;
}

let total = 0;
for (let r = 1; r <= ROUNDS; r++) total = render();
console.log(`raytrace\t${total}\t${rays}`);
