const N = Number(process.argv[2]) || 85000;

class Bus {
  constructor() { this.handlers = new Map(); this.prefixes = new Map(); this.subs = 0; this.depth = 0; }
  // ancestors of "a.b.c": "a.b.c", "a.b", "a", "*"  (cached per event name)
  chain(name) {
    let c = this.prefixes.get(name);
    if (c) return c;
    c = [name];
    let pos = name.length;
    while (true) {
      let dot;
      for (let i = pos; i >= 1; i--) {
        if (name.charAt(i - 1) === ".") { dot = i; break; }
      }
      if (!dot) break;
      c.push(name.substring(0, dot - 1));
      pos = dot - 1;
    }
    c.push("*");
    this.prefixes.set(name, c);
    return c;
  }
  on(name, fn, priority, once) {
    let list = this.handlers.get(name);
    if (!list) { list = []; this.handlers.set(name, list); }
    const h = { fn: fn, priority: priority ?? 0, once: once, live: true };
    let at = list.length;
    for (let i = 0; i < list.length; i++) {
      if (list[i].priority < h.priority) { at = i; break; }
    }
    list.splice(at, 0, h);
    this.subs = this.subs + 1;
    return h;
  }
  off(name, h) {
    const list = this.handlers.get(name);
    if (!list || !h.live) return;
    for (let i = 0; i < list.length; i++) {
      if (list[i] === h) { list.splice(i, 1); h.live = false; this.subs = this.subs - 1; return; }
    }
  }
  emit(name, ...args) {
    const chain = this.chain(name);
    this.depth = this.depth + 1;
    for (let ci = 0; ci < chain.length; ci++) {
      const list = this.handlers.get(chain[ci]);
      if (list) {
        // iterate over a snapshot: handlers may subscribe/unsubscribe while running
        const n = list.length;
        const snap = [];
        for (let i = 0; i < n; i++) snap[i] = list[i];
        for (let i = 0; i < n; i++) {
          const h = snap[i];
          if (h.live) {
            if (h.once) this.off(chain[ci], h);
            invocations = invocations + 1;
            h.fn(name, ...args);
          }
        }
      }
    }
    this.depth = this.depth - 1;
  }
}
let invocations = 0;

const bus = new Bus();
let total = 0;
const counters = new Map();
function counter(key) {
  return function (name, a, b, c) {
    counters.set(key, (counters.get(key) || 0) + 1);
    total = (total + (a ?? 0) + (b ?? 0) * 2 + (c ?? 0) * 3) % 1000003;
  };
}

const EVENTS = ["net.http.request", "net.http.response", "net.tcp.connect", "ui.click", "ui.key.down", "ui.key.up",
  "db.query.select", "db.query.insert", "timer.tick", "log.warn"];
for (let i = 1; i <= EVENTS.length; i++) {
  const ev = EVENTS[i - 1];
  bus.on(ev, counter(ev), i % 3);
  bus.on(ev, counter(ev + "#2"), 5 - i % 4);
}
bus.on("net", counter("net.any"));
bus.on("ui", counter("ui.any"));
bus.on("db.query", counter("db.query.any"));
bus.on("*", function (name, a) { total = (total + name.length + (a ?? 0)) % 1000003; }, -10);
// a handler that re-emits another event (bounded recursion) and a self-unsubscribing handler
const reh = bus.on("timer.tick", function (name, a, b) {
  if (a !== undefined && a % 50 === 0 && bus.depth < 3) bus.emit("log.warn", a, b);
});
const selfOff = bus.on("ui.click", function (name, a) { if (a !== undefined && a % 4999 === 0) bus.off("ui.click", selfOff); });

let seed = 31;
function rnd(n) { seed = (seed * 16807) % 2147483647; return seed % n + 1; }
for (let i = 1; i <= N; i++) {
  const ev = EVENTS[rnd(EVENTS.length) - 1];
  const r = rnd(10);
  if (r <= 5) {
    bus.emit(ev, i, r);
  } else if (r <= 8) {
    bus.emit(ev, i, r, i % 7);
  } else if (r === 9) {
    bus.emit(ev);
  } else {
    // transient one-shot subscription
    bus.on(ev, counter("once"), 0, true);
    bus.emit(ev, i);
  }
}
console.log(`events\t${invocations}\t${total}\t${bus.subs}\t${counters.size}`);
