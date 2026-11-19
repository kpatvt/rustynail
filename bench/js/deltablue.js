const N = Number(process.argv[2]) || 20;

class Strength {
  constructor(v, name) { this.value = v; this.name = name; }
}
const REQUIRED = new Strength(0, "required");
const STRONG_PREFERRED = new Strength(1, "strongPreferred");
const PREFERRED = new Strength(2, "preferred");
const STRONG_DEFAULT = new Strength(3, "strongDefault");
const NORMAL = new Strength(4, "normal");
const WEAK_DEFAULT = new Strength(5, "weakDefault");
const WEAKEST = new Strength(6, "weakest");
const ALL = [REQUIRED, STRONG_PREFERRED, PREFERRED, STRONG_DEFAULT, NORMAL, WEAK_DEFAULT, WEAKEST];
function stronger(a, b) { return a.value < b.value; }
function weaker(a, b) { return a.value > b.value; }
function weakestOf(a, b) { if (weaker(a, b)) return a; else return b; }

const FORWARD = 1, BACKWARD = -1, NONE = 0;
let planner;
let checks = 0;

function removeValue(t, v) {
  for (let i = 0; i < t.length; i++) {
    if (t[i] === v) { t.splice(i, 1); return; }
  }
}

class Variable {
  constructor(name, value) {
    this.name = name; this.value = value || 0; this.constraints = []; this.determinedBy = null; this.mark = 0;
    this.walkStrength = WEAKEST; this.stay = true;
  }
  addConstraint(c) { this.constraints.push(c); }
  removeConstraint(c) {
    removeValue(this.constraints, c);
    if (this.determinedBy === c) this.determinedBy = null;
  }
}

class Constraint {
  addConstraint() {
    this.addToGraph();
    planner.incrementalAdd(this);
  }
  satisfy(mark) {
    this.chooseMethod(mark);
    if (!this.isSatisfied()) {
      if (this.strength === REQUIRED) throw new Error("Could not satisfy a required constraint!");
      return null;
    }
    this.markInputs(mark);
    const out = this.output();
    const overridden = out.determinedBy;
    if (overridden !== null) overridden.markUnsatisfied();
    out.determinedBy = this;
    if (!planner.addPropagate(this, mark)) throw new Error("Cycle encountered");
    out.mark = mark;
    return overridden;
  }
  destroyConstraint() {
    if (this.isSatisfied()) planner.incrementalRemove(this); else this.removeFromGraph();
  }
  isInput() { return false; }
}

class UnaryConstraint extends Constraint {
  init(v, strength) {
    this.strength = strength;
    this.myOutput = v;
    this.satisfied = false;
    this.addConstraint();
  }
  addToGraph() {
    this.myOutput.addConstraint(this);
    this.satisfied = false;
  }
  chooseMethod(mark) {
    this.satisfied = (this.myOutput.mark !== mark) && stronger(this.strength, this.myOutput.walkStrength);
  }
  isSatisfied() { return this.satisfied; }
  markInputs(mark) { }
  output() { return this.myOutput; }
  recalculate() {
    this.myOutput.walkStrength = this.strength;
    this.myOutput.stay = !this.isInput();
    if (this.myOutput.stay) this.execute();
  }
  markUnsatisfied() { this.satisfied = false; }
  inputsKnown(mark) { return true; }
  removeFromGraph() {
    if (this.myOutput !== null) this.myOutput.removeConstraint(this);
    this.satisfied = false;
  }
}

class StayConstraint extends UnaryConstraint {
  execute() { }
}
function newStay(v, s) { const c = new StayConstraint(); c.init(v, s); return c; }

class EditConstraint extends UnaryConstraint {
  isInput() { return true; }
  execute() { }
}
function newEdit(v, s) { const c = new EditConstraint(); c.init(v, s); return c; }

class BinaryConstraint extends Constraint {
  init(v1, v2, strength) {
    this.strength = strength;
    this.v1 = v1;
    this.v2 = v2;
    this.direction = NONE;
    this.addConstraint();
  }
  chooseMethod(mark) {
    if (this.v1.mark === mark) {
      if (this.v2.mark !== mark && stronger(this.strength, this.v2.walkStrength)) {
        this.direction = FORWARD;
      } else {
        this.direction = NONE;
      }
      return;
    }
    if (this.v2.mark === mark) {
      if (this.v1.mark !== mark && stronger(this.strength, this.v1.walkStrength)) {
        this.direction = BACKWARD;
      } else {
        this.direction = NONE;
      }
      return;
    }
    if (weaker(this.v1.walkStrength, this.v2.walkStrength)) {
      if (stronger(this.strength, this.v1.walkStrength)) this.direction = BACKWARD; else this.direction = NONE;
    } else {
      if (stronger(this.strength, this.v2.walkStrength)) this.direction = FORWARD; else this.direction = BACKWARD;
    }
  }
  addToGraph() {
    this.v1.addConstraint(this);
    this.v2.addConstraint(this);
    this.direction = NONE;
  }
  isSatisfied() { return this.direction !== NONE; }
  markInputs(mark) { this.input().mark = mark; }
  input() { if (this.direction === FORWARD) return this.v1; else return this.v2; }
  output() { if (this.direction === FORWARD) return this.v2; else return this.v1; }
  recalculate() {
    const ihn = this.input(), out = this.output();
    out.walkStrength = weakestOf(this.strength, ihn.walkStrength);
    out.stay = ihn.stay;
    if (out.stay) this.execute();
  }
  markUnsatisfied() { this.direction = NONE; }
  inputsKnown(mark) {
    const i = this.input();
    return i.mark === mark || i.stay || i.determinedBy === null;
  }
  removeFromGraph() {
    if (this.v1 !== null) this.v1.removeConstraint(this);
    if (this.v2 !== null) this.v2.removeConstraint(this);
    this.direction = NONE;
  }
}

class ScaleConstraint extends BinaryConstraint {
  addToGraph() {
    super.addToGraph();
    this.scale.addConstraint(this);
    this.offset.addConstraint(this);
  }
  removeFromGraph() {
    super.removeFromGraph();
    if (this.scale !== null) this.scale.removeConstraint(this);
    if (this.offset !== null) this.offset.removeConstraint(this);
  }
  markInputs(mark) {
    super.markInputs(mark);
    this.scale.mark = mark;
    this.offset.mark = mark;
  }
  execute() {
    if (this.direction === FORWARD) {
      this.v2.value = this.v1.value * this.scale.value + this.offset.value;
    } else {
      this.v1.value = (this.v2.value - this.offset.value) / this.scale.value;
    }
  }
  recalculate() {
    const ihn = this.input(), out = this.output();
    out.walkStrength = weakestOf(this.strength, ihn.walkStrength);
    out.stay = ihn.stay && this.scale.stay && this.offset.stay;
    if (out.stay) this.execute();
  }
}
function newScale(src, scale, offset, dest, strength) {
  const c = new ScaleConstraint();
  c.scale = scale; c.offset = offset;
  c.init(src, dest, strength);
  return c;
}

class EqualityConstraint extends BinaryConstraint {
  execute() { this.output().value = this.input().value; }
}
function newEquality(a, b, s) { const c = new EqualityConstraint(); c.init(a, b, s); return c; }

class Plan {
  constructor() { this.v = []; }
  execute() {
    const v = this.v;
    for (let i = 0; i < v.length; i++) v[i].execute();
  }
}

class Planner {
  constructor() { this.currentMark = 0; }
  incrementalAdd(c) {
    const mark = this.newMark();
    let overridden = c.satisfy(mark);
    while (overridden !== null) overridden = overridden.satisfy(mark);
  }
  incrementalRemove(c) {
    const out = c.output();
    c.markUnsatisfied();
    c.removeFromGraph();
    const unsatisfied = this.removePropagateFrom(out);
    for (let sv = 0; sv <= 5; sv++) {
      const strength = ALL[sv];
      for (let i = 0; i < unsatisfied.length; i++) {
        const u = unsatisfied[i];
        if (u.strength === strength) this.incrementalAdd(u);
      }
    }
  }
  newMark() {
    this.currentMark = this.currentMark + 1;
    return this.currentMark;
  }
  makePlan(sources) {
    const mark = this.newMark();
    const plan = new Plan();
    const todo = sources;
    while (todo.length > 0) {
      const c = todo.shift();
      if (c.output().mark !== mark && c.inputsKnown(mark)) {
        plan.v.push(c);
        c.output().mark = mark;
        this.addConstraintsConsumingTo(c.output(), todo);
      }
    }
    return plan;
  }
  extractPlanFromConstraints(constraints) {
    const sources = [];
    for (let i = 0; i < constraints.length; i++) {
      const c = constraints[i];
      if (c.isInput() && c.isSatisfied()) sources.push(c);
    }
    return this.makePlan(sources);
  }
  addPropagate(c, mark) {
    const todo = [c];
    while (todo.length > 0) {
      const d = todo.pop();
      if (d.output().mark === mark) {
        this.incrementalRemove(c);
        return false;
      }
      d.recalculate();
      this.addConstraintsConsumingTo(d.output(), todo);
    }
    return true;
  }
  removePropagateFrom(out) {
    out.determinedBy = null;
    out.walkStrength = WEAKEST;
    out.stay = true;
    const unsatisfied = [];
    const todo = [out];
    while (todo.length > 0) {
      const v = todo.pop();
      const cs = v.constraints;
      for (let i = 0; i < cs.length; i++) {
        if (!cs[i].isSatisfied()) unsatisfied.push(cs[i]);
      }
      const determining = v.determinedBy;
      for (let i = 0; i < cs.length; i++) {
        const nxt = cs[i];
        if (nxt !== determining && nxt.isSatisfied()) {
          nxt.recalculate();
          todo.push(nxt.output());
        }
      }
    }
    return unsatisfied;
  }
  addConstraintsConsumingTo(v, coll) {
    const determining = v.determinedBy;
    const cc = v.constraints;
    for (let i = 0; i < cc.length; i++) {
      const c = cc[i];
      if (c !== determining && c.isSatisfied()) coll.push(c);
    }
  }
}

function chainTest(n) {
  planner = new Planner();
  let prev = null, first = null, last = null;
  for (let i = 0; i <= n; i++) {
    const v = new Variable("v" + i);
    if (prev !== null) newEquality(prev, v, REQUIRED);
    if (i === 0) first = v;
    if (i === n) last = v;
    prev = v;
  }
  newStay(last, STRONG_DEFAULT);
  const edit = newEdit(first, PREFERRED);
  const plan = planner.extractPlanFromConstraints([edit]);
  for (let i = 0; i <= 99; i++) {
    first.value = i;
    plan.execute();
    if (last.value !== i) throw new Error("Chain test failed.");
    checks = checks + 1;
  }
}

function change(v, newValue) {
  const edit = newEdit(v, PREFERRED);
  const plan = planner.extractPlanFromConstraints([edit]);
  for (let i = 1; i <= 10; i++) {
    v.value = newValue;
    plan.execute();
  }
  edit.destroyConstraint();
}

function projectionTest(n) {
  planner = new Planner();
  const scale = new Variable("scale", 10);
  const offset = new Variable("offset", 1000);
  let src = null, dst = null;
  const dests = [];
  for (let i = 0; i <= n - 1; i++) {
    src = new Variable("src" + i, i);
    dst = new Variable("dst" + i, i);
    dests.push(dst);
    newStay(src, NORMAL);
    newScale(src, scale, offset, dst, REQUIRED);
  }
  change(src, 17);
  if (dst.value !== 1170) throw new Error("Projection 1 failed");
  change(dst, 1050);
  if (src.value !== 5) throw new Error("Projection 2 failed");
  change(scale, 5);
  for (let i = 0; i <= n - 2; i++) {
    if (dests[i].value !== i * 5 + 1000) throw new Error("Projection 3 failed");
    checks = checks + 1;
  }
  change(offset, 2000);
  for (let i = 0; i <= n - 2; i++) {
    if (dests[i].value !== i * 5 + 2000) throw new Error("Projection 4 failed");
    checks = checks + 1;
  }
}

for (let i = 1; i <= N; i++) {
  chainTest(100);
  projectionTest(100);
}
console.log(`deltablue\t${checks}`);
