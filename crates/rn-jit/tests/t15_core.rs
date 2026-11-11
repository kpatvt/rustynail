//! Tier 1.5 region code generator, integer slice (task T4a; docs/JIT_TIER15.md section 15).
//!
//! * **Twin programs.** Every program runs in a jit-off interpreter and with `RUSTYNAIL_JIT=always` +
//!   `T15=always` (every nest is promoted at its first back-edge) under every allocation mode (`default`, `-ra`,
//!   `+randra=SEED`, `+flushall`) and, for a subset, under count budgets 3/17/200. Results must be identical.
//!   The programs exercise exits at arbitrary points: type changes mid-loop, overflow, break/goto, errors caught
//!   by `pcall` whose handler reads exact frame values with `debug.getlocal` (temporaries included),
//!   `debug.setlocal` from a callee mid-loop, hooks installed mid-loop, coroutine yields, closures that write
//!   captured loop variables, recursion from loops.
//! * **Stress variants** (re-run this binary with the environment): `RUSTYNAIL_GC_STRESS=1|2`,
//!   `RUSTYNAIL_JIT_STRESS=stackmove`.
//! * **Emulator tests** (Miri-runnable): the regions of hand-picked integer loops run on synthetic frames in the
//!   emulator and, outside Miri, natively; the exit state (all frame slots, budget, `savedpc`) must equal the one
//!   of the tier-1 code of the same function, for random initial values and budgets.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::t15::runtime::{OptFlags, T15Config, T15Mode};
use rn_jit::{Config, Mode, Stats};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
            "the compiler must be installed by this test binary"
        );
        if std::env::var("RUSTYNAIL_JIT_STRESS").is_ok_and(|v| v == "stackmove") {
            rn_core::jit_hooks::set_stress_stackmove(true);
        }
    });
}

fn new_state(mode: Mode, t15: T15Config) -> LuaState {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    rn_jit::set_t15(&mut st, t15);
    st
}

fn exec(st: &mut LuaState, src: &str) -> String {
    let base = st.top();
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=snippet", None),
        Status::Ok,
        "load"
    );
    let func = st.top() - 1;
    let status = st.pcall(func, LUA_MULTRET, 0);
    let v = st.get(func);
    let out = if status == Status::Ok {
        match (v.as_string(), v.as_number()) {
            (Some(s), _) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
            (None, Some(n)) => {
                String::from_utf8_lossy(rn_core::number::tostring(n).as_bytes()).into_owned()
            }
            _ => panic!("snippet must return a string or a number, got {v:?}"),
        }
    } else {
        format!(
            "ERROR {status:?}: {}",
            v.as_string().map_or_else(
                || format!("{v:?}"),
                |s| String::from_utf8_lossy(s.as_bytes()).into_owned()
            )
        )
    };
    st.set_top(base);
    norm_addr(&out)
}

/// Replaces `0x<hex>` addresses (they differ between states) by `0xADDR`.
fn norm_addr(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'0'
            && b.get(i + 1) == Some(&b'x')
            && b.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
        {
            i += 2;
            while b.get(i).is_some_and(u8::is_ascii_hexdigit) {
                i += 1;
            }
            out.push_str("0xADDR");
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

pub(crate) fn run_with(
    src: &str,
    mode: Mode,
    t15: T15Config,
    budget: Option<i32>,
) -> (String, Stats) {
    let mut st = new_state(mode, t15);
    if let Some(b) = budget {
        rn_jit::set_count_budget(&mut st, b);
    }
    let out = exec(&mut st, src);
    (out, rn_jit::stats(&mut st))
}

pub(crate) fn cfg(opt: &str) -> T15Config {
    let mut c = T15Config::for_mode(T15Mode::Always);
    c.opt = OptFlags::parse(&format!("{opt},+force"));
    assert!(c.opt.unknown.is_empty(), "{:?}", c.opt.unknown);
    c
}

const OPTS: [&str; 9] = [
    "",
    "-ra",
    "+randra=1",
    "+randra=2",
    "+randra=3",
    "+randra=7",
    "+flushall",
    "+verify",
    "+verify,+flushall,+randra=5",
];
const BUDGET_OPTS: [&str; 4] = ["", "-ra", "+randra=1", "+verify"];

/// Runs `src` in all variants and compares with the jit-off twin; returns the regions emitted by the default run.
fn twin(src: &str) -> Stats {
    let (want, s0) = run_with(src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
    assert_eq!(s0.enters, 0);
    let mut first = None;
    for o in OPTS {
        let (got, s) = run_with(src, Mode::Always, cfg(o), None);
        assert_eq!(want, got, "opt `{o}` differs for:\n{src}");
        first.get_or_insert(s);
    }
    for o in BUDGET_OPTS {
        for b in [3, 17, 200] {
            let (got, _) = run_with(src, Mode::Always, cfg(o), Some(b));
            assert_eq!(want, got, "opt `{o}`, budget {b} differs for:\n{src}");
        }
    }
    first.expect("ran")
}

// ---------------------------------------------------------------------------------------------------- programs

const INT_SUM: &str = "
    local function f(n) local s = 0 for i = 1, n do s = s + i * 3 - 1 end return s end
    local t = 0
    for k = 1, 20 do t = t + f(50) end
    return t";

const NESTED: &str = "
    local function f(n)
      local w = 0
      for i = 1, n do for j = 1, i do w = w + (i ~ j) % 5 + (i & j) end end
      return w
    end
    return f(30) + f(7)";

const WHILE_MOD: &str = "
    local function f(n)
      local x, k = 0, 0
      while k < n do k = k + 1; if k % 3 == 0 then x = x + k else x = x - 1 end end
      return x
    end
    return f(100) .. ',' .. f(1) .. ',' .. f(0)";

const LOOP_CARRIED: &str = "
    local function f(n)
      local a, b, c = 1, 2, 3
      for i = 1, n do a, b, c = b, c, a + b + c end
      return a .. ' ' .. b .. ' ' .. c
    end
    return f(10) .. '|' .. f(50) .. '|' .. f(0)";

const OVERFLOW: &str = "
    local function f(n)
      local x, y = math.maxinteger - 3, math.mininteger + 3
      local m = 3037000500
      for i = 1, n do x = x + 1; y = y - 1; m = m * m end
      return x .. ' ' .. y .. ' ' .. m
    end
    return f(8)";

const BREAK_GOTO: &str = "
    local function f(n)
      local s = 0
      for i = 1, n do
        if i > 7 then break end
        if i % 2 == 0 then goto cont end
        s = s + i
        ::cont::
      end
      local k = 0
      while true do k = k + 1 if k > n then break end end
      for i = 1, 5 do for j = 1, 5 do if i * j > 12 then goto out end s = s + i * j end end
      ::out::
      return s .. ',' .. k
    end
    return f(20)";

const REPEAT_TEST: &str = "
    local function f(n)
      local i, s, flag = 0, 0, true
      repeat
        i = i + 1; s = s + i
        if i >= n then flag = false end
      until not flag
      local j = 0
      while flag == false and j < 5 do j = j + 1 end
      return s .. ',' .. j
    end
    return f(10) .. ';' .. f(1)";

const CMP_FORMS: &str = "
    local function f(n)
      local c = 0
      for i = 1, n do
        local j = n - i
        if i < j then c = c + 1 end
        if i <= j then c = c + 2 end
        if i == 3 then c = c + 4 end
        if i ~= 4 then c = c + 8 end
        if i > 5 then c = c + 16 end
        if i >= 6 then c = c + 32 end
        if i == 9007199254740993 then c = c + 64 end
        if j > 2 then c = c + 128 elseif j < 1 then c = c + 256 end
      end
      return c
    end
    return f(12) .. ',' .. f(3)";

const NEG_STEP: &str = "
    local function f(a, b, s)
      local t = 0
      for i = a, b, s do t = t * 2 + i end
      return t
    end
    return f(10, 1, -1) .. ',' .. f(1, 10, 3) .. ',' .. f(math.maxinteger - 2, math.maxinteger, 1)
       .. ',' .. f(math.mininteger, math.mininteger + 2, 1) .. ',' .. f(5, 1, 1) .. ',' .. f(1, 3, 1.5)";

const FLOAT_MIX: &str = "
    local function f(n)
      local s, g = 0, 0.0
      for i = 1, n do s = s + i % 7 * 3 - (i >> 2); g = g + i * 0.5 end
      return s .. ' ' .. g
    end
    return f(100)";

const TABLE_LOOP: &str = "
    local function f(n)
      local t = {}
      for i = 1, n do t[i] = i * i end
      local s = 0
      for i = 1, n do s = s + t[i] - i end
      return s .. ' ' .. #t
    end
    return f(50) .. ',' .. f(3)";

const STRING_LOOP: &str = "
    local function f(n)
      local parts = {}
      for i = 1, n do parts[#parts + 1] = 'x' .. i end
      return table.concat(parts, ',')
    end
    return f(12)";

const CLOSURES: &str = "
    local function f(n)
      local c = 0
      local function inc(d) c = c + d end
      local fs = {}
      for i = 1, n do
        inc(i)
        c = c + 2
        local j = i
        fs[i] = function() j = j + 1 return j + c end
      end
      local r = {}
      for i = 1, n do r[#r + 1] = fs[i]() end
      return c .. ':' .. table.concat(r, ',')
    end
    return f(6)";

const RECURSION: &str = "
    local function r(n)
      if n == 0 then return 0 end
      local s = 0
      for i = 1, 3 do s = s + r(n - 1) + i end
      return s
    end
    return r(6) .. ',' .. r(0)";

const COROUTINE: &str = "
    local function gen(n)
      return coroutine.wrap(function()
        local a = 0
        for i = 1, n do a = a + i * 2; coroutine.yield(a + 1) end
        return -1
      end)
    end
    local g = gen(6)
    local out = {}
    for i = 1, 8 do out[#out + 1] = tostring(g()) end
    return table.concat(out, ',')";

const PCALL_IN_LOOP: &str = "
    local function f(n)
      local ok, bad, s = 0, 0, 0
      for i = 1, n do
        local r = pcall(function() if i % 3 == 0 then error('x' .. i) end return i end)
        if r then ok = ok + 1 else bad = bad + 1 end
        s = s + i * ok
      end
      return ok .. ' ' .. bad .. ' ' .. s
    end
    return f(20)";

const METAMETHOD: &str = "
    local mt = {__add = function(a, b) return setmetatable({v = (type(a) == 'table' and a.v or a) + (type(b) == 'table' and b.v or b)}, getmetatable(type(a) == 'table' and a or b)) end}
    local function f(n, start)
      local a = start
      for i = 1, n do a = a + 1 end
      return type(a) == 'table' and ('T' .. a.v) or tostring(a)
    end
    return f(5, 0) .. ',' .. f(5, setmetatable({v = 10}, mt)) .. ',' .. f(5, 0.5)";

const ERROR_HANDLER: &str = "
    local function dump(m)
      local out = {}
      for lvl = 2, 4 do
        for l = 1, 12 do
          local nm, v = debug.getlocal(lvl, l)
          if not nm then break end
          out[#out + 1] = lvl .. nm .. '=' .. tostring(v)
        end
      end
      return tostring(m):gsub('^.-:%d+: ', '') .. '|' .. table.concat(out, ',')
    end
    local function f(n, at)
      local s, c = 0, 0
      for i = 1, n do
        s = s + i
        c = c + 2 * s
        if i == at then error('boom') end
        local t = c - s
        s = s + t % 3
      end
      return s
    end
    local function g(n, at)
      local s = 1
      for i = 1, n do
        s = s * 3
        if i == at then s = s + nil end
      end
      return s
    end
    local r = {}
    for _, at in ipairs{1, 2, 5, 9} do
      r[#r + 1] = select(2, xpcall(f, dump, 12, at))
      r[#r + 1] = select(2, xpcall(g, dump, 12, at))
    end
    return table.concat(r, ';')";

const SETLOCAL_CALLEE: &str = "
    local function poke(idx, v) debug.setlocal(2, idx, v) end
    local function f(n, at, v, idx)
      local s, a = 0, 1
      for i = 1, n do
        a = a + 1
        if i == at then poke(idx, v) end
        s = s + a
      end
      return tostring(s) .. '/' .. tostring(a)
    end
    local r = {}
    -- locals of f: n(1) at(2) v(3) idx(4) s(5) a(6) then the loop's hidden slots
    for _, v in ipairs{100, 2^40, -5, 1.5, 'x', true} do
      r[#r + 1] = select(2, pcall(f, 10, 5, v, 6))
      r[#r + 1] = select(2, pcall(f, 10, 5, v, 5))
    end
    return table.concat(r, ';')";

const SETLOCAL_LOOPVAR: &str = "
    local function poke(idx, v) debug.setlocal(2, idx, v) end
    local function f(n)
      local s = 0
      for i = 1, n do
        if i == 3 then poke(5, 100) end
        s = s + i
      end
      return s
    end
    local function g(n)
      local c, s = 0, 0
      while c < n do c = c + 1; if c == 3 then poke(1, 10) end; s = s + c end
      return s .. '/' .. c
    end
    return tostring(select(2, pcall(f, 8))) .. ',' .. g(20)";

const HOOK_MIDLOOP: &str = "
    local count = 0
    local function install() debug.sethook(function() count = count + 1 end, '', 7) end
    local function f(n)
      local s = 0
      for i = 1, n do
        s = s + i
        if i == 10 then install() end
      end
      debug.sethook()
      return s
    end
    local r = f(100)
    return r .. ' ' .. count";

const LINE_HOOK: &str = "
    local lines = 0
    local function f(n)
      local s = 0
      for i = 1, n do
        s = s + i
        if i == 5 then debug.sethook(function() lines = lines + 1 end, 'l') end
      end
      debug.sethook()
      return s
    end
    return f(12) .. ' ' .. lines";

const TYPE_CHANGE: &str = "
    local function f(n, at)
      local a = 0
      for i = 1, n do
        if i == at then a = a + 0.5 end
        a = a + 1
      end
      return a
    end
    local r = {}
    for _, at in ipairs{1, 2, 3, 100, 1000} do r[#r + 1] = tostring(f(200, at)) end
    return table.concat(r, ',')";

const LIVE_IN_CHANGE: &str = "
    local function f(a, n)
      local s = 0
      for i = 1, n do a = a + 1; s = s + a end
      return s .. '/' .. a
    end
    return f(0, 5) .. ',' .. f(0.5, 5) .. ',' .. f(1, 5) .. ',' .. f(2^53, 3) .. ',' .. f(7, 0)";

const STRING_COERCE: &str = "
    local function f(a, n)
      for i = 1, n do a = a + 1 end
      return a
    end
    return tostring(f('10', 5)) .. ',' .. tostring(f(3, 5)) .. ',' .. tostring(select(2, pcall(f, {}, 2)))";

const ACCUM_BECOMES: &str = "
    local function f(n, at, v)
      local a = 0
      for i = 1, n do
        if i == at then a = v end
        a = a + 1
      end
      return a
    end
    local r = {}
    for _, v in ipairs{5, 2.5, '7', math.maxinteger} do
      for _, at in ipairs{1, 3, 50} do r[#r + 1] = tostring(select(2, pcall(f, 60, at, v))) end
    end
    return table.concat(r, ',')";

const BITOPS: &str = "
    local function f(n)
      local x, y = 0x1234, 0
      for i = 1, n do
        x = (x << 3 ~ i) & 0xffff
        y = y | (x >> 5)
        y = y ~ (i * 2654435761 & 0xffffffff)
        x = ~x & 0xffffff
        y = -y
      end
      return x .. ' ' .. y
    end
    return f(100) .. ';' .. f(3)";

const SHIFT_EDGES: &str = "
    local function f(n)
      local s = 0
      for i = 1, n do
        local x = i - 4
        s = s + (x >> 1) + (x >> 63) + (x >> 64) + (x << 62) + (x << 64) + (x >> -2) + (1 << x)
      end
      return s
    end
    return f(12)";

const MULTI_NEST: &str = "
    local function f(n)
      local a, b = 0, 0
      for i = 1, n do a = a + i end
      for j = 1, n do b = b + j * 2 end
      local k = 0
      while k < n do k = k + 1; a = a - 1 end
      for i = 1, 3 do for j = 1, 3 do for l = 1, 3 do b = b + i * j * l end end end
      return a .. ',' .. b
    end
    return f(10) .. ';' .. f(40)";

const VARARG: &str = "
    local function f(...)
      local n = select('#', ...)
      local s = 0
      for i = 1, n do s = s + (select(i, ...)) end
      return s
    end
    return f(1, 2, 3, 4, 5) .. ',' .. f()";

const TBC: &str = "
    local log = {}
    local function f(n)
      local s = 0
      for i = 1, n do
        local x <close> = setmetatable({}, {__close = function() log[#log + 1] = i end})
        s = s + i
      end
      return s .. ' ' .. #log
    end
    return f(5)";

const GOTO_LOOP: &str = "
    local function f(n)
      local i, s = 1, 0
      ::top::
      if i > n then goto done end
      s = s + i; i = i + 1
      goto top
      ::done::
      return s
    end
    return f(10)";

const GC_IN_LOOP: &str = "
    local function f(n)
      local keep = {}
      local s = 0
      for i = 1, n do
        keep[i % 7 + 1] = {i, tostring(i)}
        s = s + i
        if i % 25 == 0 then collectgarbage() end
      end
      return s .. ' ' .. #keep
    end
    return f(100)";

const GC_FINALIZER: &str = "
    local seen = 0
    local function f(n)
      local s = 0
      for i = 1, n do
        setmetatable({}, {__gc = function() seen = seen + 1 end})
        s = s + i * 2
        if i % 10 == 0 then collectgarbage() end
      end
      collectgarbage()
      return s .. ' ' .. (seen > 0 and 'gc' or 'nogc')
    end
    return f(50)";

const STACK_GROWTH: &str = "
    local function deep(n) if n == 0 then return 0 end return 1 + deep(n - 1) end
    local function f(n)
      local s = 0
      for i = 1, n do s = s + deep(i * 40) end
      return s
    end
    return f(8)";

const DIVK: &str = "
    local function f(n)
      local s = 0
      for i = -n, n do
        s = s + i % 7 + i % -7 + i // 5 + i // -3 + (i * 1000003) % 1000 + (i - 40) // 7
        if i % 4 == 0 then s = s - 1 end
      end
      local m = math.mininteger
      for i = 1, 4 do s = s + (m + i) % 3 + (m + i) // 3 + (m + i) // 2 end
      return s
    end
    return f(40)";

// ---- T4b: float / XMM programs. `H` prints the exact bits of a float (NaN payload, sign of zero), `i<n>` for integers.
const H_PRELUDE: &str = "
    local function H(x)
      if math.type(x) == 'integer' then return 'i' .. x end
      if math.type(x) ~= 'float' then return tostring(x) end
      return string.format('%016x', string.unpack('<i8', string.pack('<d', x)))
    end
";

const FL_ARITH: &str = "
    local function f(n, a, b)
      local x, y, z = a + 0.5, b * 0.25, 3
      for i = 1, n do
        x = x * 1.5 + i
        y = y - x / 3
        z = z + i * 0.5 - 1
        y = -y + z / i
        x = x - 1 + y * 2 - (i + 2) * 0.125 + 2 * y
        y = x - y
        x = i / x
      end
      return H(x) .. ' ' .. H(y) .. ' ' .. H(z)
    end
    return f(40, 1, 2) .. ',' .. f(25, 2^53, -3.5) .. ',' .. f(30, -0.0, 0.0)";

const FL_SPECIAL: &str = "
    local function f(n, a)
      local nan, inf, nz = 0.0 / 0.0, 1e308 * 10, -0.0
      local x, y, w = a * nz, 1.0, nz
      local cnt = 0
      for i = 1, n do
        x = x + nz
        y = y * inf
        if y ~= y then y = 1.0; cnt = cnt + 1 end
        if x == 0 then cnt = cnt + 10 end
        w = w * -1 + nz
        x = x / (i - 3) + y / inf
        if x < inf and x > -inf then cnt = cnt + 100 end
        y = y / 3 - inf + inf
        if y ~= y then y = nan end
        if y <= 1 or y >= 1 then cnt = cnt + 1000 end
      end
      return H(x) .. ' ' .. H(y) .. ' ' .. H(w) .. ' ' .. cnt
    end
    return f(10, 1) .. ',' .. f(7, 0) .. ',' .. f(5, -1.5)";

const FL_COMPARE: &str = "
    local function f(n, a, b)
      local nan = 0.0 / 0.0
      local c = 0
      for i = 1, n do
        local x = a + i * 0.5
        local y = b - i * 0.25
        if x < y then c = c + 1 end
        if x <= y then c = c + 2 end
        if x > y then c = c + 4 end
        if x >= y then c = c + 8 end
        if x == y then c = c + 16 end
        if x ~= y then c = c + 32 end
        if x < 3 then c = c + 64 end
        if x <= 3 then c = c + 128 end
        if x > 3 then c = c + 256 end
        if x >= 3 then c = c + 512 end
        if x == 3 then c = c + 1024 end
        if x == 2.5 then c = c + 2048 end
        if nan < x or nan == nan or nan >= y or x <= nan then c = c + 10000 end
        if not (x < nan) then c = c + 1 end
        local t = (x < y) and 1 or 2
        c = c + t
      end
      return c
    end
    return f(12, 0, 5) .. ',' .. f(9, 1, 1) .. ',' .. f(9, 2^53 - 3, 2^53) .. ',' .. f(7, 0.5, 0.5)";

// (`x == <integer constant beyond 2^53>` exits to the interpreter in tier 1: the loop never gets hot, but must be right)
const FL_EQK_BIG: &str = "
    local function f(n, a)
      local c = 0
      for i = 1, n do
        local x = a + i
        if x == 9007199254740993 then c = c + 1 end
        if x == 9007199254740992 then c = c + 10 end
        if x == 3 then c = c + 100 end
        if x == 9007199254740992.0 then c = c + 1000 end
      end
      return c
    end
    return f(5, 9007199254740988) .. ',' .. f(5, 9007199254740988.0) .. ',' .. f(5, 0.0) .. ',' .. f(5, -1.0)";

const FL_FOR: &str = "
    local function g(a, b, c, lim)
      local s, k = 0.0, 0
      for x = a, b, c do s = s + x; k = k + 1; if k >= lim then break end end
      return H(s) .. '/' .. k
    end
    local r = {}
    local cases = {
      {0, 1, 0.1}, {1, 0, -0.1}, {0.5, 10, 1.5}, {10, 0.5, -0.75}, {1, 3.5, 1}, {1.0, 3, 1}, {1, 3, 1.0},
      {0, 1/0, 1}, {0, 1e300, 1e299}, {-0.0, 2, 1}, {0, -0.0, 1}, {1, 0/0, 1}, {0/0, 1, 1}, {1, 2, 0/0},
      {2^53 - 2, 2^53 + 2, 1}, {2^53, 2^53 + 4, 1}, {math.maxinteger - 3, math.maxinteger, 1},
      {math.maxinteger - 3.0, math.maxinteger + 1.0, 1}, {math.mininteger, math.mininteger + 3, 1},
      {3, 1, 1}, {1, 3, -1}, {1, 1, 1}, {-3, 3, 2}, {3, -3, -2.5}, {1, 10, 3}, {1.5, 1.5, 1}, {1, 5, 1e-300},
      {1, 2.5, 1}, {1, 2, '1'},
    }
    for rep = 1, 3 do
      for _, c in ipairs(cases) do r[#r + 1] = select(2, pcall(g, c[1], c[2], c[3], 12)) end
    end
    r[#r + 1] = tostring(select(2, pcall(g, 1, 2, 0.0, 5)))
    r[#r + 1] = tostring(select(2, pcall(g, 1, 2, 0, 5)))
    r[#r + 1] = tostring(select(2, pcall(g, 1, 'x', 1, 5)))
    return table.concat(r, ',')";

const FL_FOR_ACCUM: &str = "
    local function f(n)
      local s, c = 0.0, 0
      for x = 0, 1, 0.1 do s = s + x; c = c + 1 end
      for x = 1, 0, -0.1 do s = s + x * 2; c = c + 1 end
      for x = 0.1, n + 0.05, 0.1 do s = s - x; c = c + 1 end
      for x = 1, 2, 0.25 do for y = x, x + 1, 0.5 do s = s + x * y end end
      return H(s) .. ' ' .. c
    end
    return f(3) .. ',' .. f(0.3)";

const FL_CONV: &str = "
    local function f(n)
      local s, t = 0.0, 0
      local big = math.maxinteger
      for i = 1, n do
        s = s + (big - i) * 1.0
        s = s - (2^53 + i)
        t = t + math.floor(2^53 - i) % 1000
        local u = (9007199254740993 + i) + 0.0
        s = s + u / 3 - (math.mininteger + i) * 0.5
        if u == 9007199254740993 + i then t = t + 1 end
        if u < 9007199254740993 + i then t = t + 2 end
      end
      return H(s) .. ' ' .. t
    end
    return f(8)";

const FL_UNM_ZERO: &str = "
    local function f(n)
      local z, o, s = 0.0, 1.0, ''
      for i = 1, n do
        local a = -z
        local b = z * -1
        local c = -(o - 1)
        local d = 0.0 - z
        local e = -a
        local f2 = z / -o
        local g = -(-0.0)
        s = H(a) .. H(b) .. H(c) .. H(d) .. H(e) .. H(f2) .. H(g)
      end
      return s
    end
    return f(5)";

const FL_COERCE: &str = "
    local function f(n, k)
      local x, y = 1.5, 0.0
      for i = 1, n do
        if i == k then x = '10' end
        y = y + x * 2
        x = x + 0.5
        if i == k then x = 10.5 end
      end
      return H(y) .. ' ' .. H(x)
    end
    local r = {}
    for _, k in ipairs{1, 2, 3, 100} do r[#r + 1] = tostring(select(2, pcall(f, 12, k))) end
    local function g(n)
      local s = 0.0
      for i = 1, n do s = s + '2' * 1.5 + (i .. '') / 2 end
      return H(s)
    end
    r[#r + 1] = g(10)
    local function e(n)
      local s = 0.0
      for i = 1, n do s = s + (i == 5 and {} or 1.5) end
      return s
    end
    r[#r + 1] = tostring(select(2, pcall(e, 10)))
    return table.concat(r, ',')";

const FL_FALLBACKS: &str = "
    local function f(n, a)
      local x, y, z = a + 0.5, 1.0, 7.25
      for i = 1, n do
        x = x + i * 0.5
        y = y + x % 3.0 + (x // 2) + z % -2.5
        y = y + x ^ 2 + math.sqrt(i) + math.floor(x)
        if y > 1e9 then y = 0.5 end
        z = z // 0.5 + x % (i + 0.5)
        x = x - 1 + 0.0 // 1
      end
      return H(x) .. ' ' .. H(y) .. ' ' .. H(z)
    end
    return f(30, 1) .. ',' .. f(20, 2^52) .. ',' .. f(10, -3.75)";

const FL_TYPE_FLIP: &str = "
    local function f(n, at)
      local a, b = 0.5, 1
      for i = 1, n do
        if i == at then a = 3 end
        if i == at + 5 then b = 2.5 end
        a = a * 2 + b
        b = b + 0.25 * i
        if a > 1e12 then a = 1 end
      end
      return H(a) .. ' ' .. H(b)
    end
    local r = {}
    for _, at in ipairs{1, 2, 3, 100} do r[#r + 1] = f(40, at) end
    return table.concat(r, ',')";

const FL_HOOK_NAN: &str = "
    local function f(n)
      local x, y = 0.5, 0.0
      local hits = 0
      debug.sethook(function() hits = hits + 1 end, '', 7)
      for i = 1, n do x = x * 1.01 + i; y = y + x / (i - 5) end
      debug.sethook()
      return H(x) .. ' ' .. H(y) .. ' ' .. (hits > 0 and 'h' or 'n')
    end
    return f(50)";

/// Float programs use the helper `H` of [`H_PRELUDE`]; the prelude is prepended to every program that mentions it.
fn with_prelude(src: &str) -> String {
    if src.contains("H(") {
        format!("{H_PRELUDE}{src}")
    } else {
        src.to_string()
    }
}

macro_rules! prog_test {
    ($($name:ident => $prog:expr,)*) => {
        $(
            #[test]
            fn $name() {
                twin(&with_prelude($prog));
            }
        )*
        /// Every fixed program above (name, source).
        const ALL_PROGS: &[(&str, &str)] = &[$((stringify!($name), $prog),)*];
    };
}

prog_test! {
    p_divk => DIVK,
    p_int_sum => INT_SUM,
    p_nested => NESTED,
    p_while_mod => WHILE_MOD,
    p_loop_carried => LOOP_CARRIED,
    p_overflow => OVERFLOW,
    p_break_goto => BREAK_GOTO,
    p_repeat_test => REPEAT_TEST,
    p_cmp_forms => CMP_FORMS,
    p_neg_step => NEG_STEP,
    p_float_mix => FLOAT_MIX,
    p_table_loop => TABLE_LOOP,
    p_string_loop => STRING_LOOP,
    p_closures => CLOSURES,
    p_recursion => RECURSION,
    p_coroutine => COROUTINE,
    p_pcall_in_loop => PCALL_IN_LOOP,
    p_metamethod => METAMETHOD,
    p_error_handler => ERROR_HANDLER,
    p_setlocal_callee => SETLOCAL_CALLEE,
    p_setlocal_loopvar => SETLOCAL_LOOPVAR,
    p_hook_midloop => HOOK_MIDLOOP,
    p_line_hook => LINE_HOOK,
    p_type_change => TYPE_CHANGE,
    p_live_in_change => LIVE_IN_CHANGE,
    p_string_coerce => STRING_COERCE,
    p_accum_becomes => ACCUM_BECOMES,
    p_bitops => BITOPS,
    p_shift_edges => SHIFT_EDGES,
    p_multi_nest => MULTI_NEST,
    p_vararg => VARARG,
    p_tbc => TBC,
    p_goto_loop => GOTO_LOOP,
    p_gc_in_loop => GC_IN_LOOP,
    p_gc_finalizer => GC_FINALIZER,
    p_stack_growth => STACK_GROWTH,
    p_fl_arith => FL_ARITH,
    p_fl_special => FL_SPECIAL,
    p_fl_compare => FL_COMPARE,
    p_fl_eqk_big => FL_EQK_BIG,
    p_fl_for => FL_FOR,
    p_fl_for_accum => FL_FOR_ACCUM,
    p_fl_conv => FL_CONV,
    p_fl_unm_zero => FL_UNM_ZERO,
    p_fl_coerce => FL_COERCE,
    p_fl_fallbacks => FL_FALLBACKS,
    p_fl_type_flip => FL_TYPE_FLIP,
    p_fl_hook_nan => FL_HOOK_NAN,
}

#[test]
fn regions_are_generated_for_integer_loops() {
    if rn_core::jit_layout::LUA54
        || std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"))
    {
        return; // T-5.5: region lowering refuses 5.4 prototypes; the graceful mutant makes the generator refuse every region
    }
    for (name, src) in [
        ("int_sum", INT_SUM),
        ("nested", NESTED),
        ("while_mod", WHILE_MOD),
        ("bitops", BITOPS),
        ("cmp_forms", CMP_FORMS),
        ("multi_nest", MULTI_NEST),
    ] {
        let (_, s) = run_with(src, Mode::Always, cfg(""), None);
        assert!(s.t15_regions >= 1, "{name}: {s:?}");
    }
}

/// The float programs run without errors, produce float regions (with `+force`, so the gate is not under test) and
/// really move values through XMM registers.
#[test]
fn regions_are_generated_for_float_loops() {
    if rn_core::jit_layout::LUA54
        || std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"))
    {
        return;
    }
    for (name, src) in [
        ("fl_arith", FL_ARITH),
        ("fl_special", FL_SPECIAL),
        ("fl_compare", FL_COMPARE),
        ("fl_for", FL_FOR),
        ("fl_for_accum", FL_FOR_ACCUM),
        ("fl_unm_zero", FL_UNM_ZERO),
        ("fl_type_flip", FL_TYPE_FLIP),
    ] {
        let src = with_prelude(src);
        let (want, _) = run_with(&src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
        if std::env::var_os("T4B_COUNT").is_some() {
            eprintln!("{name}: {want}");
        }
        assert!(!want.starts_with("ERROR"), "{name}: {want}");
        let (got, s) = run_with(&src, Mode::Always, cfg(""), None);
        assert_eq!(want, got, "{name}");
        assert!(s.t15_regions >= 1, "{name}: {s:?}");
    }
}

/// Type changes at iteration N for N in {1, 2, 3, 100}, for a live-in, a loop-carried accumulator, a value produced
/// by a fallback instruction and a table element.
#[test]
fn type_instability_at_iteration_n() {
    for n in [1, 2, 3, 100] {
        let live_in = format!(
            "local function f(a, n) for i = 1, n do a = a + 1 end return a end
             local r = {{}}
             for _, a in ipairs{{0, 0.5, 3}} do r[#r + 1] = tostring(f(a, {n})) end
             return table.concat(r, ',')"
        );
        twin(&live_in);
        let accum = format!(
            "local function f(n, at)
               local a, b = 0, 1
               for i = 1, n do
                 if i == at then a = a * 0.5 + 0.25 end
                 a = a + 1; b = b + a
               end
               return a .. ' ' .. b
             end
             return f(150, {n})"
        );
        twin(&accum);
        let from_fallback = format!(
            "local function f(n, at)
               local a, s = 1, 0
               for i = 1, n do
                 a = (i == at) and 2.5 or a
                 local t = {{a}}
                 s = s + t[1] * 2
               end
               return s
             end
             return f(150, {n})"
        );
        twin(&from_fallback);
        let element = format!(
            "local function f(t, n)
               local s = 0
               for i = 1, n do s = s + t[i] end
               return s
             end
             local t = {{}}
             for i = 1, 150 do t[i] = i end
             t[{n}] = {n} + 0.5
             return f(t, 150)"
        );
        twin(&element);
        let to_string = format!(
            "local function f(n, at)
               local a = 0
               for i = 1, n do
                 if i == at then a = '5' end
                 a = a + 1
               end
               return a
             end
             return tostring(f(150, {n}))"
        );
        twin(&to_string);
        let to_nil = format!(
            "local function f(n, at)
               local a = 0
               for i = 1, n do
                 if i == at then a = nil end
                 if a then a = a + 1 end
               end
               return tostring(a)
             end
             return f(150, {n})"
        );
        twin(&to_nil);
    }
}

/// Small generated loop bodies over four integer locals (deterministic).
fn random_sources(count: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seed = 0x1234_5678_9abc_def1u64;
    let mut rnd = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    for _ in 0..count {
        let vars = ["a", "b", "c", "d"];
        let mut body = String::new();
        for _ in 0..(3 + rnd(6)) {
            let d = vars[rnd(4) as usize];
            let x = vars[rnd(4) as usize];
            let y = vars[rnd(4) as usize];
            let k = rnd(9) as i64 - 3;
            let ops = ["+", "-", "*", "&", "|", "~"];
            let o = ops[rnd(6) as usize];
            match rnd(6) {
                0 => body += &format!("{d} = {x} {o} {y}\n"),
                1 => body += &format!("{d} = {x} {o} ({k})\n"),
                2 => body += &format!("if {x} < {y} then {d} = {d} + 1 else {d} = {d} - 2 end\n"),
                3 => body += &format!("if {x} % 3 == {} then {d} = {x} end\n", rnd(3)),
                4 => body += &format!("{d} = {x} >> {}\n", rnd(5)),
                _ => body += &format!("if {x} >= {k} then {d} = -{y} end\n"),
            }
        }
        out.push(format!(
            "local function f(n, a, b, c, d)
               for i = 1, n do
                 {body}
                 a = a + i
               end
               return a .. ',' .. b .. ',' .. c .. ',' .. d
             end
             return f(25, 1, 2, 3, 4) .. ';' .. f(3, -5, 7, 0, 9)"
        ));
    }
    out
}

/// Small generated loop bodies over float (and some integer) locals with edge-case initial values (deterministic).
fn random_float_sources(count: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut seed = 0x0bad_cafe_1357_9bdfu64;
    let mut rnd = move |n: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % n
    };
    let inits = [
        "0.0", "-0.0", "0.5", "1", "2", "-3.25", "1e308", "1/0", "-1/0", "2^53", "3", "0.1", "-7",
        "0/0",
    ];
    for _ in 0..count {
        let vars = ["a", "b", "c", "d"];
        let mut body = String::new();
        for _ in 0..(3 + rnd(7)) {
            let d = vars[rnd(4) as usize];
            let x = vars[rnd(4) as usize];
            let y = vars[rnd(4) as usize];
            let k = [0.5, 2.0, -1.5, 3.0, 0.1, 1e10][rnd(6) as usize];
            let ki = rnd(9) as i64 - 3;
            let o = ["+", "-", "*", "/"][rnd(4) as usize];
            match rnd(14) {
                0 | 1 => body += &format!("{d} = {x} {o} {y}\n"),
                2 => body += &format!("{d} = {x} {o} {k:?}\n"),
                3 => body += &format!("{d} = {x} {o} ({ki})\n"),
                4 => body += &format!("{d} = {ki} {o} {y}\n"),
                5 => body += &format!("{d} = -{x}\n"),
                6 => body += &format!("{d} = {x} {o} i\n"),
                7 => body += &format!("if {x} < {y} then {d} = {d} + 1.5 else {d} = {d} - 2 end\n"),
                8 => body += &format!("if {x} <= {k:?} then {d} = {x} end\n"),
                9 => body += &format!("if {x} == {y} then {d} = {d} * 0.5 end\n"),
                10 => body += &format!("if {x} ~= {x} then {d} = 1.25 end\n"),
                11 => body += &format!("if {x} >= {ki} then {d} = -{y} end\n"),
                12 => body += &format!("{d} = {x} % 3.5 + {y} // 2\n"),
                _ => {
                    body += &format!(
                        "if {x} > {y} then {d} = {x} elseif {x} > {ki} then {d} = {y} end\n"
                    )
                }
            }
        }
        let i0 = inits[rnd(inits.len() as u64) as usize];
        let i1 = inits[rnd(inits.len() as u64) as usize];
        let i2 = inits[rnd(inits.len() as u64) as usize];
        let i3 = inits[rnd(inits.len() as u64) as usize];
        out.push(format!(
            "{H_PRELUDE}
             local function f(n, a, b, c, d)
               for i = 1, n do
                 {body}
               end
               return H(a) .. ',' .. H(b) .. ',' .. H(c) .. ',' .. H(d)
             end
             return f(25, {i0}, {i1}, {i2}, {i3}) .. ';' .. f(3, {i3}, {i0}, 1.5, {i2})"
        ));
    }
    out
}

#[test]
fn random_float_programs() {
    for (case, src) in random_float_sources(60).iter().enumerate() {
        let (want, _) = run_with(src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
        for o in ["", "-ra", "+randra=5", "+randra=9", "+flushall", "+verify"] {
            for b in [None, Some(17)] {
                let (got, _) = run_with(src, Mode::Always, cfg(o), b);
                assert_eq!(want, got, "case {case} opt `{o}` budget {b:?}:\n{src}");
            }
        }
    }
}

#[test]
fn random_integer_programs() {
    for (case, src) in random_sources(60).iter().enumerate() {
        let (want, _) = run_with(src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
        for o in ["", "-ra", "+randra=5", "+flushall"] {
            for b in [None, Some(17)] {
                let (got, _) = run_with(src, Mode::Always, cfg(o), b);
                assert_eq!(want, got, "case {case} opt `{o}` budget {b:?}:\n{src}");
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------- stats

#[test]
fn off_mode_emits_no_region() {
    let (_, s) = run_with(
        INT_SUM,
        Mode::Always,
        T15Config::for_mode(T15Mode::Off),
        None,
    );
    assert_eq!((s.t15_regions, s.t15_recompiled), (0, 0));
}

// ------------------------------------------------------------------------------ T12: more mid-run mutation programs

const HOOK_COUNT_ONOFF: &str = "
    local count = 0
    local function on() debug.sethook(function() count = count + 1 end, '', 5) end
    local function off() debug.sethook() end
    local function f(n)
      local s, t = 0, 1
      for i = 1, n do
        s = s + i * t
        t = t + 1
        if i == 20 then on() end
        if i == 70 then off() end
      end
      return s .. ' ' .. t
    end
    return f(120) .. ' ' .. count .. ' ' .. f(10) .. ' ' .. count";

const HOOK_FROM_FINALIZER: &str = "
    local count = 0
    local function f(n)
      local s = 0
      for i = 1, n do
        if i == 15 then
          setmetatable({}, {__gc = function() debug.sethook(function() count = count + 1 end, '', 3) end})
        end
        s = s + i * 3
        if i % 5 == 0 then collectgarbage() end
        if i == 60 then debug.sethook() end
      end
      return s
    end
    return f(90) .. ' ' .. count";

const HOOK_CALL_EVENT: &str = "
    local calls = 0
    local function g(x) return x + 1 end
    local function f(n)
      local s = 0
      for i = 1, n do
        if i == 10 then debug.sethook(function(ev) calls = calls + 1 end, 'cr') end
        s = s + g(i)
        if i == 25 then debug.sethook() end
      end
      return s
    end
    return f(40) .. ' ' .. calls";

const HOOK_LINE_ONOFF: &str = "
    local lines = {}
    local function f(n)
      local s = 0
      for i = 1, n do
        s = s + i
        if i == 3 then debug.sethook(function(ev, l) lines[#lines + 1] = l end, 'l') end
        s = s * 2 % 1000003
        if i == 6 then debug.sethook() end
      end
      return s
    end
    return f(12) .. ' ' .. table.concat(lines, ',')";

const SETLOCAL_TYPES_HOOK: &str = "
    local seq = {1.5, 'x', nil, 7, 2^40, true}
    local k = 0
    local function f(n)
      local s, a = 0, 1
      for i = 1, n do
        a = a + 1
        if i == 8 then
          debug.sethook(function() k = k % #seq + 1; debug.setlocal(2, 6, seq[k]) end, '', 11)
        end
        if i == 40 then debug.sethook() end
        s = s + (tonumber(a) or 0)
      end
      return tostring(s) .. '/' .. tostring(a)
    end
    return select(2, pcall(f, 60)) .. ' ' .. k";

const UPVALUEJOIN_ALIAS: &str = "
    local function f(n)
      local a, b = 0, 0
      local function h() a = a + 100 end
      local function g() return b end
      for i = 1, n do
        a = a + 1; b = b + 2
        if i % 7 == 0 then h() end
        if i == 30 then debug.upvaluejoin(h, 1, g, 1) end
        if i == 50 then b = b + 0.5 end
      end
      return a .. ' ' .. b
    end
    return f(80)";

const SETUPVALUE_MIDLOOP: &str = "
    local function f(n)
      local a, s = 0, 0
      local function h() return a end
      for i = 1, n do
        a = a + i
        if i == 12 then debug.setupvalue(h, 1, 0.5) end
        if i == 24 then debug.setupvalue(h, 1, 'z') end
        if i == 26 then debug.setupvalue(h, 1, 1) end
        if type(a) == 'number' then s = s + a end
      end
      return tostring(a) .. ' ' .. s
    end
    return select(2, pcall(f, 40))";

const COROUTINE_HOT: &str = "
    local function gen(n, k)
      return coroutine.wrap(function()
        local a, b = 0, 1
        for i = 1, n do
          a = a + i
          b = b * 3 % 1009
          if i % k == 0 then coroutine.yield(a + b) end
          if i == n // 2 then a = a + 0.25 end
        end
        return -1
      end)
    end
    local out = {}
    for _, k in ipairs{1, 3, 50} do
      local g = gen(120, k)
      local acc = 0
      for _ = 1, 200 do local v = g(); if v == nil then break end; acc = acc + v end
      out[#out + 1] = acc
    end
    return table.concat(out, ',')";

const UNSTABLE_ACC: &str = "
    local function f(n, K)
      local acc = 0
      for i = 1, n do
        local phase = (i // K) % 4
        if phase == 0 then acc = acc + 1
        elseif phase == 1 then acc = acc * 1.5
        elseif phase == 2 then acc = tostring(acc // 1) .. ''
        else acc = nil; acc = i end
        acc = acc + 1
      end
      return tostring(acc)
    end
    local r = {}
    for _, K in ipairs{1, 7, 1000} do r[#r + 1] = select(2, pcall(f, 3000, K)) end
    return table.concat(r, ',')";

const VARIABLE_BOUNDS: &str = "
    local function f(a, b, c)
      local s = 0
      for i = a, b, c do s = s + i; if s > 1e6 then break end end
      return s
    end
    local r = {}
    local cases = {{1, 50, 1}, {1.0, 50, 1}, {1, 50.5, 1}, {1, 50, 2.0}, {50, 1, -1}, {'1', 10, 1}, {1, '10', 1},
      {1, 10, 0}, {math.maxinteger - 5, math.maxinteger, 3}, {math.mininteger, math.mininteger + 3, 1}, {1, 1/0, 1}}
    for rep = 1, 3 do
      for _, c in ipairs(cases) do r[#r + 1] = tostring(select(2, pcall(f, c[1], c[2], c[3]))) end
    end
    return table.concat(r, ',')";

const OVERFLOW_LOOPS: &str = "
    local function f(n)
      local a, b, c = math.maxinteger - 20, 1, math.mininteger + 20
      for i = 1, n do
        a = a + 3; c = c - 3; b = b * 3 + i
        if a < 0 then a = a & 0xff end
      end
      local s = 0
      for i = math.maxinteger - 40, math.maxinteger, 7 do s = s ~ i end
      for i = math.mininteger + 40, math.mininteger, -7 do s = s ~ i end
      return a .. ' ' .. b .. ' ' .. c .. ' ' .. s
    end
    return f(60)";

const CALL_BUDGET: &str = "
    local function g(x) return x + 1 end
    local function f(n)
      local s, t = 0, 0
      for i = 1, n do
        s = s + g(i)
        t = t + g(g(s)) % 7
        local u = math.abs(-i)
        s = s + u
      end
      return s .. ' ' .. t
    end
    return f(80)";

const EXTRA_PROGS: &[(&str, &str)] = &[
    ("hook_count_onoff", HOOK_COUNT_ONOFF),
    ("hook_from_finalizer", HOOK_FROM_FINALIZER),
    ("hook_call_event", HOOK_CALL_EVENT),
    ("hook_line_onoff", HOOK_LINE_ONOFF),
    ("setlocal_types_hook", SETLOCAL_TYPES_HOOK),
    ("upvaluejoin_alias", UPVALUEJOIN_ALIAS),
    ("setupvalue_midloop", SETUPVALUE_MIDLOOP),
    ("coroutine_hot", COROUTINE_HOT),
    ("unstable_acc", UNSTABLE_ACC),
    ("variable_bounds", VARIABLE_BOUNDS),
    ("overflow_loops", OVERFLOW_LOOPS),
    ("call_budget", CALL_BUDGET),
];

// ---------------------------------------------------------------------------------------------------- T5: tables
//
// Inline array / hash accesses in regions (docs/JIT_TIER15.md section 19). Every program is also run with a warm-up
// profile (`twin_prof`: `T15=on` with a tiny threshold, so loads are speculated from `prof[pc]` and the typed
// result paths are generated).

const TB_ARRAY: &str = "
    local function f(n)
      local t, u = {}, {}
      for i = 1, n do t[i] = i; u[i] = i * 0.5 end
      local acc = 0.0
      for r = 1, 3 do
        for i = 1, n do
          acc = acc + u[i] * t[i]
          t[i] = t[i] + 1
          u[i] = u[i] - 0.25
        end
      end
      local h = {}
      for i = 1, n, 7 do h[#h + 1] = H(t[i]) .. ':' .. H(u[i]) end
      return H(acc) .. ' ' .. table.concat(h, ',')
    end
    return f(40) .. '|' .. f(3)";

const TB_OOB_KEYS: &str = "
    local function f(t, n, off)
      local s = 0
      for i = 1, n do
        local v = t[i + off]
        if v then s = s + (type(v) == 'number' and v or 1) else s = s - 1 end
      end
      return s
    end
    local t = {}
    for i = 1, 20 do t[i] = i + 0.5 end
    t[7] = nil; t[13] = nil
    local r = {}
    for _, off in ipairs{0, -3, -5, -25, 5, 1000, 1 << 40, math.maxinteger - 40, math.mininteger, -(1 << 40)} do
      for rep = 1, 4 do r[#r + 1] = H(f(t, 30, off)) end
    end
    local z = {n = 1}
    z[0] = 'zero'; z[-1] = 'neg'
    local c = 0
    for i = -3, 3 do c = c + (z[i] and 1 or 0) end
    r[#r + 1] = c
    return table.concat(r, ',')";

const TB_FLOAT_KEYS: &str = "
    local t = {}
    for i = 1, 20 do t[i] = i * 3 end
    local function f(n, step)
      local s, k = 0, 1.0
      for i = 1, n do
        s = s + (t[k] or 0)
        k = k + step
      end
      return s
    end
    local function g(n, at)
      local k, s = 1, 0
      for i = 1, n do
        s = s + (t[k] or 0)
        if i == at then k = k + 0.0 end
        k = k + 1
        if k > 20 then k = 1 end
      end
      return s
    end
    local r = {}
    for rep = 1, 3 do
      r[#r + 1] = f(30, 1); r[#r + 1] = f(30, 0.5); r[#r + 1] = f(30, 2.0); r[#r + 1] = f(20, 0/0)
      r[#r + 1] = g(60, 5); r[#r + 1] = g(60, 1000)
    end
    return table.concat(r, ',')";

const TB_NIL_HOLES: &str = "
    local function f(n)
      local t = {}
      for i = 1, 30 do t[i] = i end
      local s = 0
      for r = 1, n do
        for i = 1, 30 do
          local v = t[i]
          if v == nil then s = s + 100; t[i] = i else s = s + v end
          if (i + r) % 5 == 0 then t[i] = nil end
        end
      end
      return s
    end
    return f(6) .. ',' .. f(1)";

fn fields_src(nf: usize) -> String {
    let mut decl = String::new();
    for i in 1..=nf {
        if i % 3 == 0 {
            decl += &format!("a{i} = {i}.5, ");
        } else {
            decl += &format!("a{i} = {i}, ");
        }
    }
    format!(
        "local t = {{{decl}}}
         local function f(n)
           local s = 0.0
           for i = 1, n do
             s = s + t.a1 + t.a{m} + t.a{nf} * 0.5 + t.a{q}
             t.a{m} = t.a{m} + 1
             t.a{nf} = t.a{nf} - 0.5
             s = s + (t.zz or 0)
             if i == 5 then t.newfield = 1 end
             if i == 9 then t.a{q} = 7.25 end
           end
           return H(s) .. ' ' .. H(t.a{m}) .. ' ' .. H(t.a{nf}) .. ' ' .. tostring(t.newfield)
         end
         return f(30) .. '|' .. f(30)",
        m = nf / 2 + 1,
        q = nf / 3 + 2
    )
}

const TB_FIELD_TYPE_FLIP: &str = "
    local function f(n, at, what)
      local t = {x = 1, y = 2}
      local s = 0
      for i = 1, n do
        if i == at then
          if what == 1 then t.x = '5' elseif what == 2 then t.x = 0.5 elseif what == 3 then t.x = nil
          elseif what == 4 then t.x = {} else t.x = true end
        end
        s = s + t.x + t.y
        t.y = t.y + 1
      end
      return s
    end
    local r = {}
    for what = 1, 5 do
      for _, at in ipairs{1, 2, 3, 40} do r[#r + 1] = tostring(select(2, pcall(f, 60, at, what))) end
    end
    return table.concat(r, ',')";

const TB_META_MIDLOOP: &str = "
    local function f(n, at, how)
      local t = {}
      for i = 1, 30 do t[i] = i end
      t[7] = nil
      local mt = {__index = function(_, k) return -k end,
                  __newindex = function(tt, k, v) rawset(tt, k, v * 2) end}
      local function setmt() setmetatable(t, mt) end
      local s = 0
      local fin = setmetatable({}, {__gc = function() setmetatable(t, mt) end})
      for i = 1, n do
        if i == at then
          if how == 1 then setmetatable(t, mt) elseif how == 2 then setmt()
          else fin = nil; collectgarbage() end
        end
        s = s + (t[i % 40 + 1] or 0)
        if i > 3 then s = s + (t[(i + 3) % 40 + 1] or 100) end
        t[i % 45 + 1] = i
        t[i % 30 + 1] = t[i % 30 + 1] + 1
      end
      local h = {}
      for i = 1, 46, 5 do h[#h + 1] = tostring(rawget(t, i)) end
      return s .. ' ' .. table.concat(h, ',')
    end
    local r = {}
    for how = 1, 3 do
      for _, at in ipairs{1, 2, 10, 500} do r[#r + 1] = f(60, at, how) end
    end
    return table.concat(r, '|')";

const TB_META_FIELDS: &str = "
    local function f(n, at)
      local t = {x = 1, y = 2.5}
      local log = {}
      local mt = {__newindex = function(tt, k, v) log[#log + 1] = k; rawset(tt, k, v) end,
                  __index = function(_, k) return 7 end}
      local s = 0
      for i = 1, n do
        if i == at then setmetatable(t, mt) end
        t.x = (t.x or 0) + 1
        if i % 4 == 0 then t.x = nil end
        s = s + (t.x or 0) + t.y
        if i > 3 then s = s + (t.x or 100) end
        t.y = t.y + 0.5
        if i % 6 == 0 then t.y = nil; t.y = 1.5 end
      end
      return s .. ' ' .. #log .. ' ' .. tostring(t.x)
    end
    return f(40, 1) .. '|' .. f(40, 9) .. '|' .. f(40, 1000)";

const TB_NON_TABLE: &str = "
    local function f(n, at, what)
      local t = {1, 2, 3, 4, 5, 6, 7, 8}
      local s = 0
      for i = 1, n do
        if i == at then
          local v
          if what == 1 then v = 5 elseif what == 2 then v = nil elseif what == 3 then v = 'abc'
          elseif what == 4 then v = {10, 20, 30} elseif what == 5 then v = setmetatable({}, {__index = function(_, k) return k end})
          else v = 2.5 end
          (function() debug.setlocal(2, 3, v) end)()
        end
        s = s + (t[i % 8 + 1] or 0)
      end
      return tostring(s)
    end
    local r = {}
    for what = 1, 6 do
      for _, at in ipairs{1, 2, 30} do r[#r + 1] = tostring(select(2, pcall(f, 60, at, what))) end
    end
    return table.concat(r, '|')";

const TB_CALLEE_REHASH: &str = "
    local function f(n)
      local t = {}
      for i = 1, 50 do t[i] = i end
      local s = 0
      for i = 1, n do
        s = s + (t[i % 60 + 1] or 0)
        if i % 10 == 0 then
          (function()
            for j = #t, 1, -1 do t[j] = nil end
            for j = 1, 20 + i do t[j] = j * 2 end
          end)()
        end
        if i % 13 == 0 then table.remove(t) end
        if i % 17 == 0 then table.insert(t, 3, 99) end
        t[#t + 1] = i
      end
      return s .. ' ' .. #t
    end
    return f(120)";

const TB_GROWTH: &str = "
    local function f(n)
      local t, h = {}, {}
      local s = 0
      for i = 1, n do
        t[#t + 1] = i
        t[i] = t[i] + 1
        h.a = (h.a or 0) + i
        h['k' .. (i % 5)] = i
        s = s + t[i] + h.a
      end
      return s .. ' ' .. #t .. ' ' .. h.k3
    end
    return f(200)";

const TB_NIL_STORE: &str = "
    local function f(n)
      local t = {10, 20, 30, 40}
      local p = {x = 1, y = 2}
      local s = 0
      for i = 1, n do
        t[i % 4 + 1] = nil
        p.x = nil
        s = s + (t[(i + 1) % 4 + 1] or 1) + (p.y or 0)
        t[i % 4 + 1] = i
        p.x = i
        p.y = p.y + 1
      end
      return s .. ' ' .. t[1] .. ' ' .. p.x .. ' ' .. p.y
    end
    return f(50)";

const TB_GC_BARRIER_GEN: &str = "
    collectgarbage('generational')
    local function f(n)
      local old = {}
      for i = 1, 200 do old[i] = i end
      collectgarbage()
      local keep = {}
      for i = 1, n do
        old[i % 200 + 1] = {i}
        keep.last = {i, tostring(i)}
        old[(i * 7) % 200 + 1] = tostring(i) .. 'x'
        if i % 17 == 0 then collectgarbage('step', 1) end
      end
      collectgarbage()
      local s = 0
      for i = 1, 200 do local v = old[i]; s = s + (type(v) == 'table' and v[1] or #v) end
      return s .. ' ' .. keep.last[1]
    end
    local r = f(300)
    collectgarbage('incremental')
    return r";

const TB_GC_BARRIER_INC: &str = "
    collectgarbage('incremental')
    local function f(n)
      local old = {}
      for i = 1, 200 do old[i] = {i} end
      local keep = {a = {}, b = {}}
      for i = 1, n do
        old[i % 200 + 1] = {i}
        keep.a = {i, {i}}
        keep.b = old[(i * 3) % 200 + 1]
        old[(i * 7) % 200 + 1] = function() return i end
        if i % 5 == 0 then collectgarbage('step', 2) end
      end
      collectgarbage()
      local s = 0
      for i = 1, 200 do
        local v = old[i]
        s = s + (type(v) == 'table' and v[1] or v())
      end
      return s .. ' ' .. keep.a[2][1] .. ' ' .. keep.b[1]
    end
    return f(400)";

const TB_WEAK: &str = "
    local function f(n, mode)
      local w = setmetatable({}, {__mode = mode})
      local keep = {}
      for i = 1, n do
        local v = {i}
        w[i % 20 + 1] = v
        if i % 3 == 0 then keep[#keep + 1] = v end
        if i % 11 == 0 then collectgarbage() end
      end
      collectgarbage()
      local c = 0
      for i = 1, 20 do if w[i] then c = c + 1 end end
      return c .. ' ' .. #keep
    end
    return f(80, 'v') .. '|' .. f(80, 'k') .. '|' .. f(80, 'kv')";

const TB_FINALIZER_STORE: &str = "
    local function f(n)
      local t = {}
      local log = 0
      for i = 1, n do
        t[i % 10 + 1] = setmetatable({}, {__gc = function() log = log + 1 end})
        t[(i + 3) % 10 + 1] = i
        if i % 7 == 0 then collectgarbage() end
      end
      collectgarbage()
      return log
    end
    return f(70) > 0 and 'ok' or 'none'";

const TB_COROUTINE_SHARED: &str = "
    local shared = {}
    for i = 1, 50 do shared[i] = 0 end
    local function worker(id, n)
      return coroutine.wrap(function()
        for i = 1, n do
          shared[i % 50 + 1] = shared[i % 50 + 1] + id
          shared.count = (shared.count or 0) + 1
          if i % 3 == 0 then coroutine.yield() end
        end
        return 'done'
      end)
    end
    local ws = {worker(1, 40), worker(10, 40), worker(100, 40)}
    local live = 3
    while live > 0 do
      for k = 1, 3 do
        if ws[k] then
          if ws[k]() == 'done' then ws[k] = nil; live = live - 1 end
        end
      end
    end
    local s = 0
    for i = 1, 50 do s = s + shared[i] end
    return s .. ' ' .. shared.count";

const TB_NUMERIC: &str = "
    local function mm(n)
      local a, b, c = {}, {}, {}
      for i = 1, n do
        local ra, rb = {}, {}
        for j = 1, n do ra[j] = (i + j) / n; rb[j] = (i * j % 7) / n end
        a[i], b[i] = ra, rb
      end
      for i = 1, n do
        local ai, ci = a[i], {}
        for j = 1, n do
          local s = 0.0
          for k = 1, n do s = s + ai[k] * b[k][j] end
          ci[j] = s
        end
        c[i] = ci
      end
      local tr = 0
      for i = 1, n do tr = tr + c[i][i] end
      return H(tr)
    end
    local function nbody(steps)
      local bodies = {
        {x = 0, y = 0, z = 0, vx = 0, vy = 0, vz = 0, mass = 39.47},
        {x = 4.84, y = -1.16, z = -0.10, vx = 0.606, vy = 2.81, vz = -0.02, mass = 0.037},
        {x = 8.34, y = 4.12, z = -0.40, vx = -1.01, vy = 1.82, vz = 0.008, mass = 0.011}}
      local sqrt = math.sqrt
      local dt = 0.01
      for _ = 1, steps do
        for i = 1, #bodies do
          local bi = bodies[i]
          local bix, biy, biz, bimass = bi.x, bi.y, bi.z, bi.mass
          local bivx, bivy, bivz = bi.vx, bi.vy, bi.vz
          for j = i + 1, #bodies do
            local bj = bodies[j]
            local dx, dy, dz = bix - bj.x, biy - bj.y, biz - bj.z
            local dist2 = dx * dx + dy * dy + dz * dz
            local mag = sqrt(dist2); mag = dt / (mag * dist2)
            local bm = bj.mass * mag
            bivx = bivx - dx * bm; bivy = bivy - dy * bm; bivz = bivz - dz * bm
            bm = bimass * mag
            bj.vx = bj.vx + dx * bm; bj.vy = bj.vy + dy * bm; bj.vz = bj.vz + dz * bm
          end
          bi.vx = bivx; bi.vy = bivy; bi.vz = bivz
          bi.x = bix + dt * bivx; bi.y = biy + dt * bivy; bi.z = biz + dt * bivz
        end
      end
      return H(bodies[1].x) .. H(bodies[2].vy) .. H(bodies[3].z)
    end
    local function fk(n)
      local p, q, s, sign, maxflips, sum = {}, {}, {}, 1, 0, 0
      for i = 1, n do p[i] = i; q[i] = i; s[i] = i end
      repeat
        local q1 = p[1]
        if q1 ~= 1 then
          for i = 2, n do q[i] = p[i] end
          local flips = 1
          while true do
            local qq = q[q1]
            if qq == 1 then sum = sum + sign * flips; if flips > maxflips then maxflips = flips end; break end
            q[q1] = q1
            if q1 >= 4 then local i, j = 2, q1 - 1; repeat q[i], q[j] = q[j], q[i]; i = i + 1; j = j - 1 until i >= j end
            q1 = qq; flips = flips + 1
          end
        end
        if sign == 1 then p[2], p[1] = p[1], p[2]; sign = -1
        else
          p[2], p[3] = p[3], p[2]; sign = 1
          for i = 3, n do
            local sx = s[i]
            if sx ~= 1 then s[i] = sx - 1; break end
            if i == n then return sum .. '/' .. maxflips end
            s[i] = i
            local t = p[1]; for j = 1, i do p[j] = p[j + 1] end; p[i + 1] = t
          end
        end
      until false
    end
    local function sieve(n)
      local flags = {}
      for i = 2, n do flags[i] = true end
      for i = 2, math.floor(math.sqrt(n)) do
        if flags[i] then for j = i * i, n, i do flags[j] = false end end
      end
      local c = 0
      for i = 2, n do if flags[i] then c = c + 1 end end
      return c
    end
    return mm(9) .. ' ' .. nbody(40) .. ' ' .. fk(6) .. ' ' .. sieve(500)";

const TB_VALUE_KINDS: &str = "
    local function f(n)
      local t = {}
      local o = {a = 0, b = 0, c = 0, d = 0, e = 0}
      local fn = function() return 1 end
      local s = 0
      for i = 1, n do
        t[1] = 'str'; t[2] = true; t[3] = 2.5; t[4] = i; t[5] = fn; t[6] = {}; t[7] = nil; t[8] = false
        o.a = 'x' .. (i % 3); o.b = true; o.c = 1.5; o.d = fn; o.e = false
        s = s + #t[1] + (t[2] and 1 or 0) + t[3] + t[4] + t[5]() + #o.a + (o.b and 1 or 0) + o.c + o.d()
        if t[7] == nil and t[8] == false and o.e == false and type(t[6]) == 'table' then s = s + 1 end
      end
      return H(s)
    end
    return f(40)";

const TB_ERRORS: &str = "
    local function f(n, at, what)
      local t = {x = {y = 1}, 5, 6, 7}
      local s = 0
      for i = 1, n do
        if i == at then
          if what == 1 then t = nil elseif what == 2 then t.x = nil elseif what == 3 then t = 5 else t[2] = {} end
        end
        s = s + t[1] + t.x.y
      end
      return s
    end
    local r = {}
    for what = 1, 4 do
      for _, at in ipairs{1, 3, 20} do
        local ok, e = pcall(f, 30, at, what)
        r[#r + 1] = tostring(e)
      end
    end
    return table.concat(r, '|')";

const TB_OOP: &str = "
    local C = {}
    C.__index = C
    function C.new(x) return setmetatable({x = x, y = 0}, C) end
    function C:bump(d) self.x = self.x + d; self.y = self.y + 1; return self.x end
    local function f(n)
      local o = C.new(1)
      local p = C.new(2.5)
      local s = 0
      for i = 1, n do
        s = s + o:bump(i) + p:bump(0.5)
        o.x = o.x - 1
        if i == 20 then C.bump = function(self, d) self.x = self.x + 2 * d; return self.x end end
      end
      return H(s) .. ' ' .. H(o.x) .. ' ' .. H(p.x) .. ' ' .. o.y
    end
    return f(50)";

const TB_PROXY: &str = "
    local function f(n)
      local base = {}
      for i = 1, 20 do base[i] = i end
      local sink = {}
      local px = setmetatable({}, {__index = base, __newindex = sink})
      local s = 0
      for i = 1, n do
        s = s + (px[i % 25 + 1] or 0)
        px[i % 25 + 1] = i
        rawset(px, 1, rawget(px, 1) or 0)
        s = s + (sink[i % 25 + 1] or 0)
      end
      return s .. ' ' .. #sink
    end
    return f(80)";

const TB_SPECIAL_VALUES: &str = "
    local function f(n)
      local t = {}
      local nan, nz = 0/0, -0.0
      for i = 1, n do
        t[1] = nz; t[2] = nan; t[3] = math.mininteger; t[4] = 1e308 * 10
        local a, b, c, d = t[1], t[2], t[3], t[4]
        t[5] = a; t[6] = b; t[7] = c; t[8] = d
      end
      return H(t[5]) .. H(t[6]) .. H(t[7]) .. H(t[8])
    end
    return f(30)";

const TB_STACK_MOVE: &str = "
    local function deep(n) if n == 0 then return 0 end return 1 + deep(n - 1) end
    local function f(n)
      local t = {}
      for i = 1, 30 do t[i] = i end
      local s = 0
      for i = 1, n do
        s = s + t[i % 30 + 1]
        s = s + deep(i * 60)
        t[i % 30 + 1] = s % 1000
      end
      return s
    end
    return f(8)";

const TB_SPARSE_INT: &str = "
    local function f(n)
      local t = {}
      for i = 1, 10 do t[i] = i end
      for i = 1, n do t[1000000 + i] = i end
      local s = 0
      for i = 1, n do s = s + t[1000000 + i] + (t[i] or 0) end
      return s
    end
    return f(40)";

const TB_SELF_ASSIGN: &str = "
    local function f(n)
      local t = {{1, 2}, {3, 4}, {5, 6}}
      local s = 0
      for i = 1, n do
        t = t[i % 3 + 1]
        if #t == 0 then t = {{1, 2}, {3, 4}, {5, 6}} else s = s + t[1] end
        if type(t[1]) == 'number' then t = {{t[1], t[2]}, {t[2], t[1]}, {7, 8}} end
      end
      return s
    end
    return f(30)";

// ---- T4b review: cross-register-class paths and XMM exhaustion
//
// * an integer-class slot (many integer uses) that is also written by a float act (`x = y * 1.5`): the float result
//   goes to the slot in memory, and a later float operation reads that slot through a memory operand (`fy`);
// * a slot that is the control slot of an integer `for` (payload-only stores of `FORLOOP`) and a float-class slot in a
//   deeper loop later in the same nest (the register number is reused after the first loop ends);
// * more live floats than XMM registers: the lightest ones stay slot-resident and are used as memory operands.

const CROSS_INT_SLOT_FLOAT_DEF: &str = "
    local function f(n, a)
      local x, y, z = 0, a + 0.5, 0.25
      for i = 1, n do
        x = i * 3 + x % 7
        x = x ~ i
        x = y * 1.5
        z = z + x
        z = z * x - 0.5
        x = i + 1
        y = y + z * 0.125
        if z > 1e6 or z < -1e6 then z = 0.25 end
      end
      return H(z) .. ' ' .. H(x) .. ' ' .. H(y)
    end
    return f(60, 2) .. '|' .. f(7, 0) .. '|' .. f(60, 3.5)";

const CROSS_FOR_CONTROL_FLOAT_CLASS: &str = "
    local function f(n, a)
      local s, t = 0, 0.0
      for r = 1, 3 do
        for i = 1, n do s = s + i end
        local x = a + 0.5
        for q = 1, n do
          for p = 1, n do x = x * 1.0001 + 0.5; x = x - 0.25 * x + 1.5 end
        end
        t = t + x
      end
      return s .. ' ' .. H(t)
    end
    return f(12, 1) .. '|' .. f(3, 0) .. '|' .. f(9, 2.5)";

const XMM_EXHAUSTION: &str = "
    local function f(n, a)
      local v1, v2, v3, v4, v5, v6 = a + 0.5, 1.5, 2.5, 3.5, 4.5, 5.5
      local v7, v8, v9, v10, v11, v12 = 6.5, 7.5, 8.5, 9.5, 10.5, 11.5
      local v13, v14, v15, v16, v17, v18 = 12.5, 13.5, 14.5, 15.5, 16.5, 17.5
      for i = 1, n do
        v1 = v1 * 0.999 + v2; v2 = v2 + v3 * 0.5; v3 = v3 - v4 * 0.25; v4 = v4 + v5 * 0.125
        v5 = v5 * 0.99 + v6; v6 = v6 + v7 * 0.0625; v7 = v7 - v8 * 0.03125; v8 = v8 + v9 * 0.015625
        v9 = v9 * 0.98 + v10; v10 = v10 + v11 * 0.5; v11 = v11 - v12 * 0.25; v12 = v12 + v13 * 0.125
        v13 = v13 * 0.97 + v14; v14 = v14 + v15 * 0.5; v15 = v15 - v16 * 0.25; v16 = v16 + v17 * 0.125
        v17 = v17 * 0.96 + v18; v18 = v18 - v1 * 0.0001
        if v1 > 1e9 then v1 = 1.5 end
      end
      return H(v1) .. H(v6) .. H(v12) .. H(v18)
    end
    return f(40, 1) .. '|' .. f(5, 0.25)";

/// T7 (docs/JIT_TIER15.md 21.2): programs aimed at the run-time checks of the inline table paths that the first
/// mutation round did not exercise (the reviewer's list): the key-tag check of `RegU`, the table-tag check of an
/// unproven table fact, the collectable-bit test of a value of unproven class, the table-pointer reload of `SETFIELD`,
/// the sign extension of the hash chain offset and the payload offset of constant indices.
const TB_SUBNORMAL_KEYS: &str = "
    local function f(t, n, mk)
      local s = 0
      for i = 1, n do
        local k = (mk and i * 5e-324) or i
        local v = t[k]
        s = s + (v or -1)
        t[k] = v and v + 1 or nil
      end
      return s
    end
    local t = {}
    for i = 1, 40 do t[i] = i * 10 end
    local r = {}
    for rep = 1, 4 do
      r[#r + 1] = f(t, 30, false)
      r[#r + 1] = f(t, 30, true)
      r[#r + 1] = f(t, 45, rep % 2 == 0)
    end
    -- small-payload non-integer keys: floats with a small bit pattern / a string
    local function g(t, n, which)
      local s = 0
      for i = 1, n do
        local k
        if which == 1 then k = (i % 2 == 0) and i or (i * 5e-324)
        elseif which == 2 then k = (i % 3 == 0) and i or (i + 0.5)
        else k = (i % 2 == 0) and i or tostring(i) end
        s = s + (t[k] or 0)
      end
      return s
    end
    for rep = 1, 3 do for w = 1, 3 do r[#r + 1] = g(t, 36, w) end end
    return table.concat(r, ',')";

const TB_TABLE_TAG: &str = "
    local function f(n, w)
      local t
      if w == 1 then t = {1, 2, 3, 4, 5, 6, 7, 8}
      elseif w == 2 then t = 5
      elseif w == 3 then t = 1
      elseif w == 4 then t = {a = 1, b = 2, c = 3}
      elseif w == 5 then t = true
      else t = 'str' end
      local s = 0
      for i = 1, n do
        s = s + (t[i % 8 + 1] or 0)
      end
      return s
    end
    local function g(n, w)
      local t
      if w == 1 then t = {a = 1, b = 2, c = 3}
      elseif w == 2 then t = 3
      elseif w == 3 then t = 2.5
      else t = {} end
      local s = 0
      for i = 1, n do
        s = s + (t.a or 0)
        if w == 1 then t.b = t.b + 1 end
      end
      return s
    end
    local function h(n, w)
      local t
      if w == 1 then t = {0, 0, 0, 0}
      elseif w == 2 then t = 2
      elseif w == 3 then t = 1 else t = {} end
      for i = 1, n do t[i % 4 + 1] = i end
      return t
    end
    local r = {}
    for rep = 1, 3 do
      for w = 1, 6 do r[#r + 1] = tostring(select(2, pcall(f, 20, w))) end
      for w = 1, 4 do r[#r + 1] = tostring(select(2, pcall(g, 20, w))) end
      for w = 1, 4 do
        local ok, e = pcall(h, 20, w)
        r[#r + 1] = type(e) == 'table' and #e or tostring(e)
      end
    end
    return table.concat(r, '|')";

const TB_MAYBE_COLL: &str = "
    collectgarbage('generational')
    local function f(n, mix)
      local old = {}
      for i = 1, 64 do old[i] = 0 end
      local fld = {a = 0, b = 0, c = 0}
      collectgarbage()
      collectgarbage()
      for i = 1, n do
        local v
        if mix == 1 then v = (i % 3 == 0) and {i} or i
        elseif mix == 2 then v = (i % 2 == 0) and 'x' .. i or 7
        else v = (i % 5 == 0) and 1.5 or i end
        old[i % 64 + 1] = v
        fld.a = v
        fld.b = (i % 4 == 0) and {i} or i
        fld.c = v
      end
      collectgarbage()
      local s = 0
      for i = 1, 64 do local x = old[i]; s = s + (type(x) == 'table' and x[1] or type(x) == 'string' and #x or x) end
      local fa = fld.a
      return s .. ' ' .. (type(fa) == 'table' and fa[1] or tostring(fa))
    end
    local r = {}
    for rep = 1, 3 do for mix = 1, 3 do r[#r + 1] = f(120, mix) end end
    collectgarbage('incremental')
    return table.concat(r, ',')";

fn chain_walk_src(nf: usize) -> String {
    // one access per key (all keys, in two orders), so that every node of every collision chain is reached,
    // including chain offsets (`next`) of both signs
    let mut decl = String::new();
    let mut get = String::new();
    let mut set = String::new();
    for i in 1..=nf {
        decl += &format!("k{i}_x = {i}, ");
        get += &format!("s = s + t.k{i}_x\n             ");
        set += &format!("t.k{i}_x = t.k{i}_x + 1\n             ");
    }
    let mut rget = String::new();
    for i in (1..=nf).rev() {
        rget += &format!("s = s + t.k{i}_x\n             ");
    }
    format!(
        "local t = {{{decl}}}
         local function f(n)
           local s = 0
           for r = 1, n do
             {get}{set}{rget}
           end
           return s
         end
         return f(6) .. ',' .. f(6)"
    )
}

const TB_CONST_INDEX: &str = "
    local function f(n)
      local t = {10, 20, 30, 40, 50, 60}
      local s = 0
      for i = 1, n do
        s = s + t[1] + t[2] * 2 + t[3] * 3 + t[6]
        t[2] = t[1] + i
        t[6] = t[5] - i
        t[4] = t[4] + 1
      end
      return s .. ' ' .. t[1] .. ' ' .. t[2] .. ' ' .. t[3] .. ' ' .. t[4] .. ' ' .. t[5] .. ' ' .. t[6]
    end
    return f(25) .. '|' .. f(3)";

const TB_PROGS: &[(&str, &str)] = &[
    ("tb_array", TB_ARRAY),
    ("tb_oob_keys", TB_OOB_KEYS),
    ("tb_float_keys", TB_FLOAT_KEYS),
    ("tb_nil_holes", TB_NIL_HOLES),
    ("tb_field_type_flip", TB_FIELD_TYPE_FLIP),
    ("tb_meta_midloop", TB_META_MIDLOOP),
    ("tb_meta_fields", TB_META_FIELDS),
    ("tb_non_table", TB_NON_TABLE),
    ("tb_callee_rehash", TB_CALLEE_REHASH),
    ("tb_growth", TB_GROWTH),
    ("tb_nil_store", TB_NIL_STORE),
    ("tb_gc_barrier_gen", TB_GC_BARRIER_GEN),
    ("tb_gc_barrier_inc", TB_GC_BARRIER_INC),
    ("tb_weak", TB_WEAK),
    ("tb_finalizer_store", TB_FINALIZER_STORE),
    ("tb_coroutine_shared", TB_COROUTINE_SHARED),
    ("tb_numeric", TB_NUMERIC),
    ("tb_value_kinds", TB_VALUE_KINDS),
    ("tb_errors", TB_ERRORS),
    ("tb_oop", TB_OOP),
    ("tb_proxy", TB_PROXY),
    ("tb_special_values", TB_SPECIAL_VALUES),
    ("tb_stack_move", TB_STACK_MOVE),
    ("tb_sparse_int", TB_SPARSE_INT),
    ("tb_self_assign", TB_SELF_ASSIGN),
    ("tb_subnormal_keys", TB_SUBNORMAL_KEYS),
    ("tb_table_tag", TB_TABLE_TAG),
    ("tb_maybe_coll", TB_MAYBE_COLL),
    ("tb_const_index", TB_CONST_INDEX),
];

const CROSS_PROGS: &[(&str, &str)] = &[
    ("cross_int_slot_float_def", CROSS_INT_SLOT_FLOAT_DEF),
    (
        "cross_for_control_float_class",
        CROSS_FOR_CONTROL_FLOAT_CLASS,
    ),
    ("xmm_exhaustion", XMM_EXHAUSTION),
];

#[test]
fn cross_register_class_and_xmm_exhaustion_programs() {
    for (name, src) in CROSS_PROGS {
        let src = with_prelude(src);
        let (want, _) = run_with(&src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
        assert!(!want.starts_with("ERROR"), "{name}: {want}");
        twin(&src);
        if !rn_core::jit_layout::LUA54
            && !std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"))
        {
            let (_, s) = run_with(&src, Mode::Always, cfg(""), None);
            assert!(s.t15_regions >= 1, "{name}: {s:?}");
        }
    }
}

pub(crate) fn table_sources() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = TB_PROGS
        .iter()
        .map(|(n, p)| ((*n).to_string(), with_prelude(p)))
        .collect();
    // hash parts of several sizes: collision chains, a node vector of 4 .. 64
    for nf in [4usize, 9, 17, 40] {
        v.push((format!("tb_fields{nf}"), with_prelude(&fields_src(nf))));
    }
    // every node of every collision chain (T7)
    for nf in [7usize, 20, 45] {
        v.push((format!("tb_chain{nf}"), chain_walk_src(nf)));
    }
    v
}

/// The profiled configuration: `T15=on` with a tiny threshold, so that `prof[pc]` is filled by the warm-up code and the
/// typed result paths of the table accesses are generated.
pub(crate) fn cfg_prof(opt: &str, hot: i32) -> T15Config {
    let mut c = T15Config::for_mode(T15Mode::On);
    c.hot = hot;
    c.opt = OptFlags::parse(&format!("{opt},+force"));
    assert!(c.opt.unknown.is_empty(), "{:?}", c.opt.unknown);
    c
}

/// Runs `src` with a warm-up profile in the allocation modes and compares with the jit-off twin.
fn twin_prof(src: &str) {
    let (want, _) = run_with(src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
    for o in [
        "",
        "-ra",
        "+randra=2",
        "+randra=5",
        "+flushall",
        "+verify",
        "-tables",
    ] {
        for hot in [2, 9] {
            let (got, _) = run_with(src, Mode::Always, cfg_prof(o, hot), None);
            assert_eq!(
                want, got,
                "profiled opt `{o}` hot {hot} differs for:\n{src}"
            );
        }
    }
    for o in ["", "+randra=1"] {
        for b in [3, 17] {
            let (got, _) = run_with(src, Mode::Always, cfg_prof(o, 3), Some(b));
            assert_eq!(
                want, got,
                "profiled opt `{o}` budget {b} differs for:\n{src}"
            );
        }
    }
}

#[test]
fn table_programs_match_the_interpreter() {
    for (name, src) in table_sources() {
        eprintln!("table program {name}");
        let (want, _) = run_with(&src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
        assert!(
            !want.starts_with("ERROR"),
            "{name}: the program itself fails: {want}"
        );
        twin(&src);
        twin_prof(&src);
    }
}

/// The T7 gap programs alone (so that the mutants of the reviewer's list can be checked against exactly these).
#[test]
fn t7_table_gap_programs() {
    for (name, src) in table_sources() {
        if name.starts_with("tb_subnormal")
            || name.starts_with("tb_table_tag")
            || name.starts_with("tb_maybe")
            || name.starts_with("tb_const_index")
            || name.starts_with("tb_chain")
        {
            let (want, _) = run_with(&src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
            assert!(!want.starts_with("ERROR"), "{name}: {want}");
            twin(&src);
            twin_prof(&src);
        }
    }
}

/// A non-singleton or nil speculated load result keeps the fallback (the inline form checks one tag byte only):
/// with a `def_widen` of `INT|FLT` for the load, no inline typed access may be generated for it (docs 21.2).
#[test]
fn widened_load_result_is_not_inlined_unchecked() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    // elements are ints and floats alternating; the profile sees whichever came last
    let src = with_prelude(
        "local t = {}
         for i = 1, 40 do t[i] = (i % 2 == 0) and i or i + 0.5 end
         local function f(n)
           local s = 0.0
           for r = 1, n do for i = 1, 40 do s = s + t[i] end end
           return H(s)
         end
         return f(5) .. ',' .. f(7)",
    );
    twin(&src);
    twin_prof(&src);
}

/// The inline paths are really generated.
#[test]
fn table_regions_are_generated() {
    if rn_core::jit_layout::LUA54
        || std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"))
    {
        return;
    }
    for (name, src) in [("tb_array", TB_ARRAY), ("tb_numeric", TB_NUMERIC)] {
        let src = with_prelude(src);
        let (_, s) = run_with(&src, Mode::Always, cfg_prof("", 3), None);
        assert!(s.t15_regions >= 1, "{name}: {s:?}");
    }
}

// ---- T6: calls and intrinsics in regions (docs/JIT_TIER15.md section 20)

/// Every intrinsic on typed float and integer operands in one hot loop, results used in typed arithmetic.
const IN_MIX: &str = "
    local sqrt, floor, ceil, abs, min, max = math.sqrt, math.floor, math.ceil, math.abs, math.min, math.max
    local sin, cos, exp, log = math.sin, math.cos, math.exp, math.log
    local function f(n)
      local s, x, k = 0.0, 0.5, 3
      local t = 0
      for i = 1, n do
        x = x * 1.37 + 0.11
        if x > 1e6 then x = x - 1e6 end
        local r = sqrt(x) + abs(x) - min(x, 2.25) * max(x, 3.5)
        local fl = floor(x) + ceil(x)
        local ia = abs(k - i) + min(i, k) + max(i, k)
        s = s + r + fl + ia + sin(x) + cos(x) + exp(x * 1e-5) + log(x + 1)
        t = t + floor(i) + ceil(k) + sqrt(i)
      end
      return H(s) .. H(t)
    end
    return f(200)";

/// The special values of the language through every intrinsic: the operand is a constant so that the facts are
/// typed; the generated chunks run 30 iterations each.
const IN_SPECIAL: &str = "
    local vals = {0.0, -0.0, 0.5, -0.5, 1.5, -1.5, 2.5, 1e15, -1e15, 2^53, 2^62, 2^63, -2^63, -2^64, 1e300, -1e300,
      1/0, -1/0, 0/0, 3.0, 1e-300, 9007199254740993.0, 0, 1, -1, 7, 1 << 62, math.mininteger, math.maxinteger}
    local function lit(v)
      if v ~= v then return '(0/0)' elseif v == 1/0 then return '(1/0)' elseif v == -1/0 then return '(-1/0)'
      elseif v == math.mininteger then return 'math.mininteger' end
      if math.type(v) == 'float' then
        if v == 0 and 1/v < 0 then return '(-0.0)' end
        local t = string.format('%.17g', v)
        return (t:find('[%.eEn]') and t or t .. '.0')
      end
      return tostring(v)
    end
    local out = {}
    local function put(v) out[#out + 1] = H(v) end
    for _, v in ipairs(vals) do
      for _, fname in ipairs{'sqrt', 'floor', 'ceil', 'abs', 'sin', 'cos', 'exp', 'log'} do
        local src = 'local put = ...; local f = math.' .. fname .. '; local x = ' .. lit(v) ..
          '; local r for i = 1, 30 do r = f(x); put(r); put(r + 1) end'
        assert(load(src))(put)
      end
      for _, w in ipairs(vals) do
        for _, fname in ipairs{'min', 'max'} do
          local src = 'local put = ...; local f = math.' .. fname .. '; local x, y = ' .. lit(v) .. ', ' .. lit(w) ..
            '; local r for i = 1, 30 do r = f(x, y); put(r) end'
          assert(load(src))(put)
        end
      end
    end
    return #out .. ' ' .. table.concat(out, ',', 1, 200) .. ' ' .. out[#out]";

/// The callee changes under the loop: the upvalue is rebound, `math.sqrt` is replaced, in the middle of the loop and
/// from a callee.
const IN_SHADOW: &str = "
    local f = math.sqrt
    local function rebind() f = function(x) return x * 2 end end
    local function g(n, at)
      local s = 0.0
      for i = 1, n do
        if i == at then rebind() end
        s = s + f(i + 0.5)
      end
      return s
    end
    local r1 = g(100, 1000)
    local r2 = g(100, 40)
    f = math.sqrt
    local function h(n, at)
      local s = 0.0
      for i = 1, n do
        if i == at then math.sqrt = function(x) return -x end end
        s = s + math.sqrt(i + 0.25)
      end
      return s
    end
    local sq = math.sqrt
    local r3 = h(100, 1000)
    local r4 = h(100, 33)
    math.sqrt = sq
    local mn = math.min
    local function m(n, at)
      local s = 0
      for i = 1, n do
        if i == at then mn = function(a, b) return a + b end end
        s = s + mn(i, 50)
      end
      return s
    end
    return H(r1) .. H(r2) .. H(r3) .. H(r4) .. H(m(100, 1000)) .. H(m(100, 60))";

/// Hooks installed in the middle of the loop: call, count and line hooks (the interpreter must see every call).
const IN_HOOK: &str = "
    local sqrt, floor = math.sqrt, math.floor
    local calls, counts, lines = 0, 0, 0
    local function f(n, kind, at)
      local s = 0.0
      for i = 1, n do
        if i == at then
          if kind == 'call' then debug.sethook(function() calls = calls + 1 end, 'c')
          elseif kind == 'count' then debug.sethook(function() counts = counts + 1 end, '', 5)
          else debug.sethook(function() lines = lines + 1 end, 'l') end
        end
        s = s + sqrt(i + 0.5) + floor(s)
      end
      debug.sethook()
      return s
    end
    local a = f(60, 'call', 20)
    local b = f(60, 'count', 20)
    local c = f(60, 'line', 20)
    local d = f(60, 'call', 1000)
    return H(a) .. H(b) .. H(c) .. H(d) .. ' ' .. calls .. ' ' .. counts .. ' ' .. lines";

/// The operand changes type at iteration K (string coercion, nil, table, boolean): errors and coercions must be the
/// real function's.
const IN_ERRORS: &str = "
    local sqrt, floor, abs, min = math.sqrt, math.floor, math.abs, math.min
    local function f(n, at, v)
      local s = 0.0
      local x = 2.5
      for i = 1, n do
        if i == at then x = v end
        s = s + sqrt(x) + floor(x) + abs(x) + min(x, 4.0)
      end
      return s
    end
    local res = {}
    for _, v in ipairs{'4', '0x10', ' 7 ', 'abc', true, {}, 9, -3} do
      for _, at in ipairs{3, 25} do
        local ok, r = pcall(f, 30, at, v)
        res[#res + 1] = tostring(ok) .. ':' .. (ok and H(r) or (tostring(r):gsub('0x%x+', '0x')))
      end
    end
    local ok, r = pcall(f, 30, 10, nil)
    res[#res + 1] = tostring(ok) .. ':' .. tostring(r)
    return table.concat(res, ' ')";

/// Overflow edges of floor/ceil and min/max with equal / NaN / signed-zero operands inside typed loops.
const IN_EDGES: &str = "
    local floor, ceil, min, max, abs = math.floor, math.ceil, math.min, math.max, math.abs
    local out = {}
    local function put(v) out[#out + 1] = H(v) end
    local function f(x, y, n)
      for i = 1, n do
        local a, b, c, d, e = floor(x), ceil(x), min(x, y), max(x, y), abs(x)
        if i == n then put(a); put(b); put(c); put(d); put(e) end
      end
    end
    for _, x in ipairs{0.0, -0.0, 0.5, -0.5, 2^62, 2^62 - 1024, 2^63 - 1024, 2^63, -2^63, -2^63 - 2048, 1/0, 0/0, -1/0} do
      for _, y in ipairs{0.0, -0.0, 1/0, 0/0, x} do
        f(x, y, 12)
      end
    end
    for _, x in ipairs{0, 1, math.mininteger, math.maxinteger, -5} do
      for _, y in ipairs{0, math.mininteger, math.maxinteger, x} do
        f(x, y, 12)
      end
    end
    return table.concat(out, ',')";

/// Intrinsic calls with a loop-carried integer and float state in registers across the call; the callee is a Lua
/// function half of the time (the flat call protocol next to inline calls).
const IN_LUA_CALLS: &str = "
    local sqrt = math.sqrt
    local function sq(x) return x * x end
    local function f(n)
      local a, b, c = 0, 0.0, 1
      for i = 1, n do
        local g = (i % 2 == 0) and sqrt or sq
        a = a + i
        b = b + g(i + 0.0)
        c = c + a % 7
      end
      return H(a) .. H(b) .. H(c)
    end
    local function h(n)
      local acc = 0.0
      for i = 1, n do acc = acc + sq(i) * sqrt(acc + i) end
      return acc
    end
    return f(150) .. H(h(60))";

/// Coroutines yielding from loops with intrinsics.
const IN_COROUTINE: &str = "
    local sqrt, floor = math.sqrt, math.floor
    local co = coroutine.wrap(function(n)
      local s = 0.0
      for i = 1, n do
        s = s + sqrt(i + 1.0) + floor(s)
        if i % 7 == 0 then coroutine.yield(s) end
      end
      return -s
    end)
    local out = {}
    local v = co(50)
    while v >= 0 do out[#out + 1] = H(v); v = co() end
    out[#out + 1] = H(v)
    return table.concat(out, ',')";

/// Garbage collection and finalizers around intrinsic loops (the slow path allocates; GC steps happen in helpers).
const IN_GC: &str = "
    local sqrt = math.sqrt
    local function f(n)
      local s = 0.0
      local keep = {}
      for i = 1, n do
        s = s + sqrt(i + 0.5)
        keep[i % 8 + 1] = {s, tostring(i)}
        if i % 25 == 0 then collectgarbage() end
      end
      return H(s) .. #keep
    end
    return f(120)";

/// `GETUPVAL` of an upvalue that `debug.upvaluejoin` made an alias of a register-resident local of the running frame
/// (before the loop, and in the middle of it): the inline upvalue load must see the register's value.
const IN_UPVALUEJOIN: &str = "
    local u = 10
    local f
    f = function(n, at)
      local x = 0
      local function g() return x end
      local s = 0
      for i = 1, n do
        if i == at then
          for k = 1, 10 do
            local nm = debug.getupvalue(f, k)
            if nm == 'u' then debug.upvaluejoin(f, k, g, 1) end
          end
        end
        x = x + 1
        s = s + u
      end
      return s, x, g()
    end
    local a, b, c = f(120, 1)
    local a2, b2, c2 = f(120, 60)
    local a3, b3, c3 = f(120, 1000)
    return H(a) .. H(b) .. H(c) .. H(a2) .. H(b2) .. H(c2) .. H(a3) .. H(b3) .. H(c3) .. H(u)";

/// Results of Lua calls feed typed arithmetic (speculated from the profile, re-guarded by the stub behind the call):
/// the callee returns a float, an integer, a string, nil, a different type every K calls.
const IN_CALL_RESULT: &str = "
    local mode = 0
    local function g(x)
      if mode == 0 then return x * 0.5
      elseif mode == 1 then return x + 1
      elseif mode == 2 then return '2.5'
      elseif mode == 3 then return nil
      else return x // 1 end
    end
    local function f(n, at, m2)
      local a, b, c = 0, 0.0, 0
      for i = 1, n do
        if i == at then mode = m2 end
        local r = g(i)
        a = a + i
        b = b + r * 2
        c = c + a % 5
      end
      mode = 0
      return a, b, c
    end
    local out = {}
    for m2 = 0, 4 do
      for _, at in ipairs{1, 30, 1000} do
        local ok, a, b, c = pcall(f, 60, at, m2)
        out[#out + 1] = tostring(ok) .. ':' .. (ok and (H(a) .. H(b) .. H(c)) or (tostring(a):gsub('0x%x+', '0x')))
      end
    end
    return table.concat(out, ' ')";

/// Multiple results and a callee that yields.
const IN_CALL_MULTI: &str = "
    local function two(x) return x * 0.5, x + 1 end
    local co = coroutine.wrap(function(x) while true do x = coroutine.yield(x * 1.5) end end)
    local function f(n)
      local s, t = 0.0, 0
      for i = 1, n do
        local a, b = two(i)
        s = s + a + co(i + 0.5)
        t = t + b
      end
      return H(s) .. H(t)
    end
    return f(80)";

const IN_PROGS: &[(&str, &str)] = &[
    ("in_call_result", IN_CALL_RESULT),
    ("in_call_multi", IN_CALL_MULTI),
    ("in_upvaluejoin", IN_UPVALUEJOIN),
    ("in_mix", IN_MIX),
    ("in_special", IN_SPECIAL),
    ("in_shadow", IN_SHADOW),
    ("in_hook", IN_HOOK),
    ("in_errors", IN_ERRORS),
    ("in_edges", IN_EDGES),
    ("in_lua_calls", IN_LUA_CALLS),
    ("in_coroutine", IN_COROUTINE),
    ("in_gc", IN_GC),
];

/// Intrinsic call sites exist only when the warm-up profile saw the callee: profiled runs (`twin_prof`) generate the
/// inline forms, `T15=always` runs mostly the ordinary call.
#[test]
fn intrinsic_programs_match_the_interpreter() {
    for (name, src) in IN_PROGS {
        let src = with_prelude(src);
        let (want, _) = run_with(&src, Mode::Off, T15Config::for_mode(T15Mode::Off), None);
        assert!(
            !want.starts_with("ERROR"),
            "{name}: the program itself fails: {want}"
        );
        twin(&src);
        twin_prof(&src);
    }
}

/// The inline forms are really generated (profiled run).
#[test]
fn intrinsic_regions_are_generated() {
    if rn_core::jit_layout::LUA54
        || std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"))
    {
        return;
    }
    let src = with_prelude(IN_MIX);
    let (_, s) = run_with(&src, Mode::Always, cfg_prof("", 3), None);
    assert!(s.t15_regions >= 1, "{s:?}");
}

#[test]
fn mid_run_mutation_programs() {
    for (_name, src) in EXTRA_PROGS {
        twin(&with_prelude(src));
    }
}

/// Every program the suite knows (fixed, mid-run mutation, random).
pub(crate) fn corpus() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = ALL_PROGS
        .iter()
        .chain(EXTRA_PROGS)
        .map(|(n, p)| ((*n).to_string(), with_prelude(p)))
        .collect();
    for (n, p) in IN_PROGS {
        v.push(((*n).to_string(), with_prelude(p)));
    }
    for (i, s) in random_sources(60).into_iter().enumerate() {
        v.push((format!("random{i}"), s));
    }
    for (i, s) in random_float_sources(60).into_iter().enumerate() {
        v.push((format!("randomf{i}"), s));
    }
    v
}

/// [`corpus`] plus the table, cross-register-class and intrinsic programs: everything `t15_stress.rs` runs under the
/// verification options.
#[allow(dead_code)] // (used by t15_stress.rs only)
pub(crate) fn stress_corpus() -> Vec<(String, String)> {
    let mut v = corpus();
    v.extend(table_sources());
    for (n, p) in CROSS_PROGS {
        v.push(((*n).to_string(), with_prelude(p)));
    }
    v
}

/// C1 (docs/JIT_TIER15.md 4.9) is exact: the bytecodes executed by compiled code (`Stats::insns`, the budget consumed)
/// must be the same with and without regions, under every count budget, for every allocation mode.
#[test]
fn instruction_counts_are_identical_with_and_without_regions() {
    for (name, src) in corpus() {
        for b in [None, Some(5), Some(37)] {
            let (out0, s0) = run_with(&src, Mode::Always, T15Config::for_mode(T15Mode::Off), b);
            for o in ["", "-ra", "+randra=3", "+flushall", "+verify,+randra=9"] {
                let (out1, s1) = run_with(&src, Mode::Always, cfg(o), b);
                assert_eq!(out0, out1, "{name}: output, opt `{o}`, budget {b:?}");
                assert_eq!(
                    s0.insns, s1.insns,
                    "{name}: instruction count differs (regions {}), opt `{o}`, budget {b:?}",
                    s1.t15_regions
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------- mutants

/// Mutation testing (docs/JIT_TIER15.md 16.5): `cargo test -p rn-jit --features t15_mutants --test t15_core`.
/// Each mutant of `src/t15/mutants.rs` is a deliberate bug in the region generator; the rest of this binary (and the
/// emulator tests of `t15_core_emu.rs`, included below) is re-run in a child process with `RUSTYNAIL_JIT_MUT=name` and
/// must fail (a crash or a hang counts as detected). Graceful mutants must still pass.
#[cfg(feature = "t15_mutants")]
#[path = "t15_core_emu.rs"]
mod emu_tests;

#[cfg(feature = "t15_mutants")]
mod mutants {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// Bugs: the suite must fail.
    const BUGS: &[&str] = &[
        "count_after_fall",
        "no_budget_reload_after_call",
        "drop_guard_first",
        "drop_guard_last",
        "drop_guard_after_fall",
        "flush_skip_last",
        "flush_skip_tag",
        "leave_no_budget",
        "pad_no_irq_poll",
        "fall_no_budget",
        "stub_no_load_last",
        "flt_flush_int_tag",
        "fcmp_negated",
        "i2f_missing",
        "fneg_missing",
        "fforloop_always_pos",
        "fforloop_clobber_idx",
        "fbin_alias_commute",
        "tbl_no_lower_bound",
        "tbl_no_upper_bound",
        "tbl_no_tag_check",
        "tbl_no_nil_check",
        "tbl_no_meta_check",
        "tbl_no_nonnil_check",
        "tbl_no_barrier",
        "tbl_no_key_tag",
        "tbl_no_table_tag",
        "tbl_no_coll_test",
        "tbl_no_s0_reload",
        "tbl_next_zext",
        "tbl_const_off",
        "pol_countdown_64bit",
        "intr_no_hook_check",
        "intr_no_identity",
        "intr_wrong_identity",
        "intr_no_stack_check",
        "intr_no_ci_check",
        "intr_no_count",
        "intr_abs_no_clear",
        "intr_minmax_swap",
        "intr_minmax_order",
        "intr_floor_ceil_swap",
        "intr_floor_no_range",
        "up_no_spill",
        "pol_no_budget_reload_stay",
        "pol_count_all_slow",
        "exitmap_wrong_reg",
        "leave_drop_entry",
        "fall_succ_no_guards",
        "rr_join_union",
    ];
    /// Not bugs of the emitted code: the generator must degrade (refuse the region) and everything still passes.
    const GRACEFUL: &[&str] = &["bad_fall_targets"];

    /// `Some(passed)`; `None` = timed out.
    fn child(mutant: Option<&str>) -> (Option<bool>, String) {
        let exe = std::env::current_exe().expect("exe");
        let mut cmd = Command::new(exe);
        cmd.args([
            "--skip",
            "every_mutant_is_detected",
            "--test-threads=4",
            "--quiet",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        cmd.env_remove("RUSTYNAIL_JIT_MUT");
        if let Some(m) = mutant {
            cmd.env("RUSTYNAIL_JIT_MUT", m);
        }
        let mut c = cmd.spawn().expect("spawn");
        let t0 = Instant::now();
        loop {
            match c.try_wait().expect("wait") {
                Some(st) => {
                    let out = c.wait_with_output().expect("output");
                    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
                    text.push_str(&String::from_utf8_lossy(&out.stderr));
                    return (Some(st.success()), text);
                }
                None if t0.elapsed() > Duration::from_secs(60) => {
                    let _ = c.kill();
                    let _ = c.wait();
                    return (None, String::new());
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    #[test]
    fn every_mutant_is_detected() {
        if rn_core::jit_layout::LUA54 {
            return; // no regions in 5.4 builds
        }
        let t0 = Instant::now();
        let (ok, text) = child(None);
        eprintln!("unmutated suite: {ok:?} in {:?}", t0.elapsed());
        assert_eq!(ok, Some(true), "the unmutated suite must pass:\n{text}");
        let mut undetected = Vec::new();
        for m in BUGS {
            let t0 = Instant::now();
            let (ok, text) = child(Some(m));
            eprintln!("mutant {m}: {ok:?} in {:?}", t0.elapsed());
            match ok {
                Some(true) => undetected.push(*m),
                Some(false) | None => {
                    let first = text
                        .lines()
                        .find(|l| {
                            l.contains("panicked")
                                || l.contains("internal error")
                                || l.contains("signal")
                        })
                        .unwrap_or("(crash or timeout)");
                    eprintln!("mutant {m}: detected ({first})");
                }
            }
        }
        for m in GRACEFUL {
            let (ok, text) = child(Some(m));
            assert_eq!(
                ok,
                Some(true),
                "graceful mutant {m} must be survived:\n{text}"
            );
        }
        assert!(
            undetected.is_empty(),
            "mutants NOT detected: {undetected:?}"
        );
    }
}
