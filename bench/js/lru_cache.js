// Same cache structure as the Lua program (map + doubly linked list of entries, state in closures).  JS has no
// __index/__newindex/__call, so the three metamethod entry points become plain methods get/put/call.
const N = Number(process.argv[2]) || 90000;

class Cache {
  constructor(capacity, onEvict) {
    let map = new Map(), head = null, tail = null, size = 0;
    let hits = 0, misses = 0, evictions = 0;
    function unlink(e) {
      if (e.prev) e.prev.next = e.next; else head = e.next;
      if (e.next) e.next.prev = e.prev; else tail = e.prev;
      e.prev = null; e.next = null;
    }
    function pushFront(e) {
      e.next = head;
      if (head) head.prev = e;
      head = e;
      if (!tail) tail = e;
    }
    this.get = function (k) {
      const e = map.get(k);
      if (e) {
        hits = hits + 1;
        if (e !== head) { unlink(e); pushFront(e); }
        return e.value;
      }
      misses = misses + 1;
      return undefined;
    };
    this.put = function (k, v) {
      let e = map.get(k);
      if (e) {
        e.value = v;
        if (e !== head) { unlink(e); pushFront(e); }
        return;
      }
      e = { key: k, value: v, prev: null, next: null };
      map.set(k, e);
      pushFront(e);
      size = size + 1;
      if (size > capacity) {
        const victim = tail;
        unlink(victim);
        map.delete(victim.key);
        size = size - 1;
        evictions = evictions + 1;
        if (onEvict) onEvict(victim.key, victim.value);
      }
    };
    this.size = function () { return size; };
    this.stats = function () { return [hits, misses, evictions]; };
  }
  call(k, compute) {  // memoised lookup
    let v = this.get(k);
    if (v === undefined) {
      v = compute(k);
      this.put(k, v);
    }
    return v;
  }
}

let seed = 7;
function rnd(n) { seed = (seed * 16807) % 2147483647; return seed % n; }
function nextKey() {
  const a = rnd(150), b = rnd(150);
  const k = a * b % 20000;
  if (k % 5 === 0) return "key" + k;
  return k;
}

let evictedSum = 0;
const caches = [
  new Cache(64, function (k, v) { evictedSum = evictedSum + v; }),
  new Cache(512, function (k, v) { evictedSum = evictedSum + v % 7; }),
  new Cache(4096),
];
let computed = 0;
function compute(k) {
  computed = computed + 1;
  if (typeof k === "string") return k.length * 13;
  return k % 1013 + 1;
}

let check = 0;
for (let i = 1; i <= N; i++) {
  const k = nextKey();
  const c = caches[i % 3];
  if (i % 11 === 0) {
    c.put(k, i % 100);
  } else {
    check = (check + c.call(k, compute)) % 1000003;
  }
  if (i % 97 === 0) {
    const v = c.get(k);
    if (v) check = (check + v) % 1000003;
  }
}
let hits = 0, misses = 0, ev = 0;
for (let i = 0; i < 3; i++) {
  const [h, m, e] = caches[i].stats();
  hits += h; misses += m; ev += e;
}
console.log(`lru_cache\t${hits}\t${misses}\t${ev}\t${computed}\t${evictedSum}\t${check}`);
