// Port of bench/corpus/classic_vector.lua: a particle system with springs over an immutable-style 2D vector class.
// The Lua operators (metamethods __add/__sub/__mul/__div/__unm/__eq/__lt) are plain methods here; the class chain
// (Object -> Particle -> HeavyParticle) is a JS class hierarchy, `p:is(T)` is `instanceof`.
const N = Number(process.argv[2]) || 1000;
class Vec {
  constructor(x, y) { this.x = x || 0; this.y = y || 0; }
  add(b) { return new Vec(this.x + b.x, this.y + b.y); }
  sub(b) { return new Vec(this.x - b.x, this.y - b.y); }
  mul(b) { return new Vec(this.x * b, this.y * b); }      // vector * number
  dot(b) { return this.x * b.x + this.y * b.y; }          // vector * vector (unused by the workload, as in Lua)
  div(b) { return new Vec(this.x / b, this.y / b); }
  unm() { return new Vec(-this.x, -this.y); }
  eq(b) { return this.x === b.x && this.y === b.y; }
  lt(b) { return this.x < b.x || (this.x === b.x && this.y < b.y); }
  le(b) { return this.x <= b.x && this.y <= b.y; }
  len() { return Math.sqrt(this.x * this.x + this.y * this.y); }
  len2() { return this.x * this.x + this.y * this.y; }
  normalized() {
    const l = this.len();
    if (l > 0) return this.div(l);
    return new Vec(0, 0);
  }
  clone() { return new Vec(this.x, this.y); }
  toString() { return "(" + this.x.toFixed(3) + ", " + this.y.toFixed(3) + ")"; }
}
class Particle {
  constructor(pos, mass) {
    this.pos = pos;
    this.vel = new Vec(0, 0);
    this.force = new Vec(0, 0);
    this.mass = mass;
  }
  applyForce(f) { this.force = this.force.add(f); }
  integrate(dt) {
    const acc = this.force.div(this.mass);
    this.vel = this.vel.add(acc.mul(dt)).mul(0.995);
    this.pos = this.pos.add(this.vel.mul(dt));
    this.force = new Vec(0, 0);
  }
  toString() { return "Object"; }   // Lua: Object.__tostring is inherited by Particle (copied by extend)
}
class HeavyParticle extends Particle {
  constructor(pos, mass) { super(pos, mass * 4); }
}
class Spring {
  constructor(a, b, k) {
    this.a = a; this.b = b; this.k = k;
    this.rest = b.pos.sub(a.pos).len();
  }
  apply() {
    const d = this.b.pos.sub(this.a.pos);
    const len = d.len();
    if (len === 0) return;
    const f = d.normalized().mul((len - this.rest) * this.k);
    this.a.applyForce(f);
    this.b.applyForce(f.unm());
  }
}

const particles = [], springs = [];
const W = 12;
for (let y = 1; y <= W; y++) {
  for (let x = 1; x <= W; x++) {
    const heavy = (x + y) % 5 === 0;
    const pos = new Vec(x * 10, y * 10);
    particles.push(heavy ? new HeavyParticle(pos, 1) : new Particle(pos, 1));
  }
}
for (let y = 1; y <= W; y++) {
  for (let x = 1; x <= W; x++) {
    const i = (y - 1) * W + x - 1;   // 0-based index of Lua's particles[i]
    if (x < W) springs.push(new Spring(particles[i], particles[i + 1], 0.8));
    if (y < W) springs.push(new Spring(particles[i], particles[i + W], 0.8));
  }
}
const gravity = new Vec(0, 0.5);
let ordered = 0;
for (let step = 1; step <= N; step++) {
  for (const s of springs) s.apply();
  for (let i = 0; i < particles.length; i++) {
    const p = particles[i];
    if (i + 1 > W) {
      p.applyForce(gravity.mul(p.mass));
      p.integrate(0.1);
    }
  }
  if (particles[0].pos.lt(particles[1].pos)) ordered = ordered + 1;
  if (particles[4].pos.eq(particles[4].pos.clone())) ordered = ordered + 1;
}
let c = new Vec(0, 0);
for (const p of particles) c = c.add(p.pos);
c = c.div(particles.length);
let heavy = 0;
for (const p of particles) if (p instanceof HeavyParticle) heavy = heavy + 1;
console.log(`centroid\t${c}\tordered\t${ordered}\theavy\t${heavy}`);
console.log(`${particles[particles.length - 1]}\t${particles[W + 4]}`);
