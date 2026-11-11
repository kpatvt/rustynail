//! Tier 1.5 policy (task T7; docs/JIT_TIER15.md section 21): stub-failure feedback, respecialisation (retarget, then
//! widen), the benign-rate penalty, the peel exemption, give-up after the respecialisation cap, versioned retirement.
//!
//! * **Twins.** Every program runs in a jit-off interpreter and with regions under many option sets
//!   (`+exithot=N` makes the policy decide after N stub failures, so the decisions happen inside these tiny programs) and
//!   count budgets; the results must be identical. The instruction-count twin checks that respecialised versions
//!   count exactly the bytecodes the interpreter does.
//! * **Policy assertions** (`Stats::t15_*`): which decisions were taken for which program shape.
//! * **Lifetime.** Versions retired while activations are suspended in them (recursion, coroutines, `pcall`),
//!   prototypes freed with live versions, code-cap pressure.
//! * **Hooks.** Compiled code (and so region code) is never entered while a count or line hook is installed.
//! * **Mutants** (cargo feature `t15_mutants`): each policy mutant must make this binary fail.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, State, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::value::Value;
use rn_jit::t15::runtime::{
    test_event, test_info, test_set_fail, EventKind, NestPhase, OptFlags, T15Config, T15Mode,
};
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

/// `pq(what)`: tier 1.5 state of the calling Lua function (0 current version, 1 retired blocks, 2 promoted nests,
/// 3 nests that are off).
fn pq(st: &mut State) -> LuaResult<usize> {
    let what = st.arg(0).as_int().unwrap_or(-1);
    let v = match test_info(st) {
        None => -1,
        Some(i) => match what {
            0 => i64::from(i.cur),
            1 => i.retired as i64,
            2 => i
                .phases
                .iter()
                .filter(|p| **p == NestPhase::Promoted)
                .count() as i64,
            3 => i.phases.iter().filter(|p| **p == NestPhase::Off).count() as i64,
            4 => i64::from(*i.fails.first().unwrap_or(&0)),
            _ => -2,
        },
    };
    st.push(Value::int(v));
    Ok(1)
}

/// `pev(ver, nest, pc)`: a `StubFail` event from the pad of code version `ver` (what a stale version's pad does).
fn pev(st: &mut State) -> LuaResult<usize> {
    let ver = st.arg(0).as_int().unwrap_or(0) as u32;
    let nest = st.arg(1).as_int().unwrap_or(0) as u32;
    let pc = st.arg(2).as_int().unwrap_or(0) as u32;
    let r = test_event(st, ver, nest, EventKind::StubFail, pc);
    st.push(Value::int(r.map_or(-1, |r| i64::from(r.ret))));
    Ok(1)
}

/// `pset(v)`: sets the failure countdown of nest 0.
fn pset(st: &mut State) -> LuaResult<usize> {
    let v = st.arg(0).as_int().unwrap_or(0) as i32;
    test_set_fail(st, 0, v);
    st.push(Value::int(0));
    Ok(1)
}

fn new_state(mode: Mode, t15: T15Config) -> LuaState {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    rn_jit::set_t15(&mut st, t15);
    let g = st.globals();
    st.raw_set_str(g, b"pq", Value::native(pq)).unwrap();
    st.raw_set_str(g, b"pev", Value::native(pev)).unwrap();
    st.raw_set_str(g, b"pset", Value::native(pset)).unwrap();
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

fn run_with(src: &str, mode: Mode, t15: T15Config, budget: Option<i32>) -> (String, Stats) {
    let mut st = new_state(mode, t15);
    if let Some(b) = budget {
        rn_jit::set_count_budget(&mut st, b);
    }
    let out = exec(&mut st, src);
    (out, rn_jit::stats(&mut st))
}

fn off() -> T15Config {
    T15Config::for_mode(T15Mode::Off)
}

/// `T15=always` (nests are promoted at their first back-edge) with `opt`; `+force` is always added.
fn cfg(opt: &str) -> T15Config {
    let mut c = T15Config::for_mode(T15Mode::Always);
    c.opt = OptFlags::parse(&format!("{opt},+force"));
    assert!(c.opt.unknown.is_empty(), "{:?}", c.opt.unknown);
    c
}

/// `T15=on` with a small threshold, so that the warm-up profile exists when the nest is promoted.
fn cfg_on(opt: &str, hot: i32) -> T15Config {
    let mut c = T15Config::for_mode(T15Mode::On);
    c.hot = hot;
    c.opt = OptFlags::parse(&format!("{opt},+force"));
    assert!(c.opt.unknown.is_empty(), "{:?}", c.opt.unknown);
    c
}

const H_PRELUDE: &str = "
    local function H(x)
      if math.type(x) == 'integer' then return 'i' .. x end
      if math.type(x) ~= 'float' then return tostring(x) end
      return string.format('%016x', string.unpack('<i8', string.pack('<d', x)))
    end
";

fn with_prelude(src: &str) -> String {
    format!("{H_PRELUDE}{src}")
}

const OPTS: [&str; 8] = [
    "+exithot=1",
    "+exithot=2",
    "+exithot=5",
    "+exithot=2,-ra",
    "+exithot=3,+randra=3",
    "+exithot=2,+flushall",
    "+exithot=3,+verify",
    "+exithot=2,+verify,+flushall,+randra=5",
];

/// Runs `src` against the jit-off twin in every option set, with the warm-up profile, and under count budgets.
/// Returns the stats of the `+exithot=2` run.
fn twin(src: &str) -> Stats {
    let (want, s0) = run_with(src, Mode::Off, off(), None);
    assert_eq!(s0.enters, 0);
    assert!(
        !want.starts_with("ERROR"),
        "the program itself fails: {want}"
    );
    let mut keep = None;
    for o in OPTS {
        let (got, s) = run_with(src, Mode::Always, cfg(o), None);
        assert_eq!(want, got, "opt `{o}` differs for:\n{src}");
        if o == "+exithot=2" {
            keep = Some(s);
        }
    }
    for o in ["+exithot=2", "+exithot=3,+randra=2", "+exithot=2,-ra"] {
        for hot in [2, 9] {
            let (got, _) = run_with(src, Mode::Always, cfg_on(o, hot), None);
            assert_eq!(want, got, "T15=on hot {hot} opt `{o}` differs for:\n{src}");
        }
    }
    for o in ["+exithot=2", "+exithot=2,+randra=1"] {
        for b in [3, 17] {
            let (got, _) = run_with(src, Mode::Always, cfg(o), Some(b));
            assert_eq!(want, got, "budget {b} opt `{o}` differs for:\n{src}");
        }
    }
    keep.expect("ran")
}

// ---------------------------------------------------------------------------------------------------- programs

/// A live-in whose type alternates between calls every `K` calls (int / float): the speculation of `s` at the loop head
/// fails (not a peel failure) until the policy retargets and then widens it.
fn flip_src(k: u32) -> String {
    with_prelude(&format!(
        "local function h(n, v)
           local s = v
           for i = 1, n do s = s + i end
           return s
         end
         local r = 0
         for c = 1, 2500 do
           local v = ((c // {k}) % 2 == 0) and 1 or 0.5
           r = r + h(40, v)
         end
         return H(r)"
    ))
}

/// The type changes once (call 80) and stays.
const ONCE: &str = "
    local function h(n, v)
      local s = v
      for i = 1, n do s = s + i end
      return s
    end
    local r = 0
    for c = 1, 300 do
      r = r + h(40, c < 80 and 1 or 0.5)
    end
    return H(r)";

/// Many types in turn (numbers, numeric strings, a table with `__add`, errors caught).
const MANY: &str = "
    local mt = {__add = function(a, b) return 7 end}
    local function h(n, v)
      local s = v
      for i = 1, n do s = s + i end
      return s
    end
    local vals = {1, 0.5, '7', 2^53, 3, setmetatable({}, mt), 1 << 62, -0.0, 'x', true, 10}
    local r = {}
    for c = 1, 220 do
      local ok, e = pcall(h, 30, vals[c % #vals + 1])
      r[#r + 1] = ok and H(e) or 'E'
    end
    return table.concat(r, ',')";

/// Array elements change type under a load whose result was speculated (def-site feedback).
fn def_flip_src(k: u32) -> String {
    with_prelude(&format!(
        "local t = {{}}
         for i = 1, 40 do t[i] = i end
         local function sum(n)
           local s = 0
           for i = 1, n do s = s + t[i] end
           return s
         end
         local r = 0
         for c = 1, 260 do
           if c % {k} == 0 then
             for i = 1, 40 do t[i] = math.type(t[i]) == 'integer' and t[i] + 0.25 or math.floor(t[i]) end
           end
           r = r + sum(40)
         end
         return H(r)"
    ))
}

/// A table live-in that is sometimes not a table, and a key live-in that is sometimes a float or a string: after the
/// respecialisation the inline accesses check the tags at run time (`tp = false`, `RegU`).
const TABLE_AND_KEY_FLIPS: &str = "
    local function get(n, t, k)
      local s = 0
      for i = 1, n do
        local v = t[k]
        s = s + (v or 0)
        t[k] = (v or 0) + 1
      end
      return s
    end
    local function fld(n, t)
      local s = 0
      for i = 1, n do
        s = s + (t.a or 1)
        if i % 3 == 0 then t.a = (t.a or 0) + 1 end
      end
      return s
    end
    local r = {}
    local a = {10, 20, 30, 40}
    local b = {a = 5, b = 6}
    for c = 1, 160 do
      local m = c % 8
      local t, k
      if m == 0 then t, k = a, 2 elseif m == 1 then t, k = a, 2.0 elseif m == 2 then t, k = 5, 1
      elseif m == 3 then t, k = a, 5e-324 elseif m == 4 then t, k = 'str', 1 elseif m == 5 then t, k = a, '2'
      elseif m == 6 then t, k = a, 3 else t, k = nil, 1 end
      local ok, e = pcall(get, 12, t, k)
      r[#r + 1] = ok and tostring(e) or 'E'
      local u = (m % 3 == 0) and 7 or (m % 3 == 1 and b or {a = 2.5})
      local ok2, e2 = pcall(fld, 9, u)
      r[#r + 1] = ok2 and tostring(e2) or 'E'
    end
    return table.concat(r, ',')";

/// The accumulator is an integer on entry and a float after the first iteration, in a nest that is entered many
/// times. Promotion happens at a back-edge, where the accumulator already is a float, so the first version
/// speculates `FLT` and fails at every entry; the first policy decision retargets it to the entry type (`INT`), which
/// makes the slot a *peel* slot (`INT` observed, `FLT` loop-carried): its first-entry failure is by design and must not
/// be counted any more.
const PEEL: &str = "
    local function h(n)
      local a = 0
      for i = 1, n do a = a + i * 0.5 end
      return a
    end
    local r = 0.0
    for c = 1, 600 do r = r + h(8) end
    return H(r)";

/// A rare failure (one call in 400 has another type) among many good entries: benign, no respecialisation.
fn rare_src() -> String {
    with_prelude(
        "local function h(n, v)
           local s = v
           for i = 1, n do s = s + i end
           return s > 40 and 1 or 0
         end
         local r = 0
         for c = 1, 1200 do r = r + h(10, c % 400 == 399 and 0.5 or 1) end
         return H(r)",
    )
}

/// Four live-ins change type one after the other, each in its own phase: every phase invalidates the current
/// speculation (the new version's slot types are fixed after each decision), so the nest runs through retarget,
/// widen, widen and finally hits the respecialisation cap: given up.
fn phases_src() -> String {
    with_prelude(
        "local function h(n, a, b, c, d)
           local s = 0
           for i = 1, n do s = s + a + b + c + d + i end
           return s
         end
         local r = 0
         local vals = {1, 1, 1, 1}
         for phase = 0, 4 do
           for call = 1, 60 do
             r = r + h(12, vals[1], vals[2], vals[3], vals[4])
           end
           if phase < 4 then vals[phase + 1] = vals[phase + 1] + 0.5 end
         end
         return H(r)",
    )
}

/// Both loops of a function are promoted and both flip: versions are bounded by the compile cap.
fn many_versions_src() -> String {
    let mut body = String::new();
    for l in 1..=6 {
        body += &format!(
            "local s{l} = v{l}
             for i = 1, n do s{l} = s{l} + i end
             r = r + s{l}
             "
        );
    }
    with_prelude(&format!(
        "local function h(n, v1, v2, v3, v4, v5, v6)
           local r = 0
           {body}
           return r
         end
         local r = 0
         for c = 1, 200 do
           local x = (c // 5 % 2 == 0) and 1 or 0.5
           local y = (c // 3 % 2 == 0) and 1 or 0.25
           r = r + h(10, x, y, x, y, y, x)
         end
         return H(r)"
    ))
}

/// Retirement while activations are suspended in older versions: recursion (the outer activations are inside a call
/// in version N while the inner ones trigger the decisions), `pcall` frames, and a coroutine suspended in the loop.
fn retire_src() -> String {
    with_prelude(
        "local function h(depth, v)
           local s = v
           for i = 1, 24 do
             s = s + i
             if i == 6 and depth > 0 then
               local ok, r = pcall(h, depth - 1, (depth % 2 == 0) and 1 or 0.5)
               s = s + (ok and r or 0)
             end
           end
           return s
         end
         local r = {}
         for c = 1, 30 do r[#r + 1] = H(h(3, (c % 3 == 0) and 0.5 or 1)) end
         local function co(v)
           local s = v
           for i = 1, 20 do
             s = s + i
             if i % 5 == 0 then coroutine.yield(s) end
           end
           return s
         end
         local cs = {}
         for k = 1, 4 do cs[k] = coroutine.create(co) end
         local out = {}
         -- interleave: the coroutines are suspended inside the loop while other calls respecialise it
         for round = 1, 5 do
           for k = 1, 4 do
             local ok, v = coroutine.resume(cs[k], (k % 2 == 0) and 1 or 0.5)
             out[#out + 1] = H(v)
             for j = 1, 8 do out[#out + 1] = H(h(1, (j + round + k) % 2 == 0 and 1 or 0.5)) end
           end
         end
         return table.concat(r, ',') .. '|' .. table.concat(out, ',')",
    )
}

/// An error raised inside a respecialised region, and a callee that changes a local with `debug.setlocal` (the next
/// stub fails and counts).
const ERRORS_AND_SETLOCAL: &str = "
    local function h(n, v, at)
      local s = v
      for i = 1, n do
        if i == at then
          (function() debug.setlocal(2, 2, 0.5) end)()
        end
        s = s + i
        if s ~= s then error('nan') end
      end
      return s
    end
    local r = {}
    for c = 1, 120 do
      local ok, e = pcall(h, 20, (c % 7 < 3) and 1 or 2.5, c % 5 == 0 and 7 or 99)
      r[#r + 1] = ok and H(e) or ('E' .. tostring(e))
    end
    return table.concat(r, ',')";

/// The profile says `INT` for a load; then the table has only holes (the stub after the load fails with `nil`: the first
/// decision *retargets* the speculation to `nil`); then the elements are young tables that are stored into an old
/// table. A nil speculation must not let the inline load commit them unchecked: the store would skip the GC barrier
/// (`Coll::No`) and a young collection would free the only referenced tables (docs 21.2, the T5-review finding).
const NIL_RETARGET: &str = "
    collectgarbage('generational')
    local function f(src, dst, n)
      for i = 1, n do
        local k = i % 8 + 1
        local v = src[k]
        dst[k] = v
      end
    end
    local ints, holes, tabs, dst = {1, 2, 3, 4, 5, 6, 7, 8}, {1, 2, 3, 4, 5, 6, 7, 8}, {}, {}
    for k = 1, 8 do holes[k] = nil; tabs[k] = false; dst[k] = 0 end
    collectgarbage(); collectgarbage()
    f(ints, dst, 60)
    f(holes, dst, 60)
    f(holes, dst, 60)
    for k = 1, 8 do tabs[k] = {k, 'x' .. k} end
    f(tabs, dst, 40)
    for k = 1, 8 do tabs[k] = false end
    collectgarbage('step', 0)
    local junk = 0
    for i = 1, 3000 do local t = {i, i + 1}; junk = junk + #t end
    collectgarbage('step', 0)
    for i = 1, 3000 do local t = {i, i + 1}; junk = junk + #t end
    local out = {}
    for k = 1, 8 do local v = dst[k]; out[#out + 1] = type(v) == 'table' and (v[1] .. v[2]) or tostring(v) end
    collectgarbage('incremental')
    return table.concat(out, ',')";

/// heapsort's shape: a `while true do ... break ... end` loop in a closure that reads the array through an upvalue (nest
/// selection of T7: the closing `JMP` is dead code behind the `break`s).
const WHILE_TRUE_SIFT: &str = "
    local n = 300
    local a, seed = {}, 7
    for i = 1, n do seed = (seed * 1103515245 + 12345) % 2147483648; a[i] = seed end
    local function sort(a, n)
      local function sift(lo, hi)
        local root = lo
        while true do
          local child = root * 2
          if child > hi then break end
          if child < hi and a[child] < a[child + 1] then child = child + 1 end
          if a[root] < a[child] then a[root], a[child] = a[child], a[root]; root = child else break end
        end
      end
      for i = n // 2, 1, -1 do sift(i, n) end
      for i = n, 2, -1 do a[1], a[i] = a[i], a[1]; sift(1, i - 1) end
    end
    sort(a, n)
    local ok = true
    for i = 2, n do if a[i - 1] > a[i] then ok = false end end
    return tostring(ok) .. ' ' .. a[1] .. ' ' .. a[n]";

fn programs() -> Vec<(String, String)> {
    let mut v = vec![
        ("flip1".to_string(), flip_src(1)),
        ("flip7".to_string(), flip_src(7)),
        ("flip1000".to_string(), flip_src(1000)),
        ("once".to_string(), with_prelude(ONCE)),
        ("many".to_string(), with_prelude(MANY)),
        ("def_flip1".to_string(), def_flip_src(1)),
        ("def_flip7".to_string(), def_flip_src(7)),
        ("def_flip1000".to_string(), def_flip_src(1000)),
        ("table_key".to_string(), TABLE_AND_KEY_FLIPS.to_string()),
        ("while_true_sift".to_string(), WHILE_TRUE_SIFT.to_string()),
        ("nil_retarget".to_string(), NIL_RETARGET.to_string()),
        ("peel".to_string(), with_prelude(PEEL)),
        ("rare".to_string(), rare_src()),
        ("phases".to_string(), phases_src()),
        ("many_versions".to_string(), many_versions_src()),
        ("retire".to_string(), retire_src()),
        (
            "errors_setlocal".to_string(),
            with_prelude(ERRORS_AND_SETLOCAL),
        ),
    ];
    // the three type-unstable accumulator shapes of the jit-diff generator, here with the policy active
    v.push(("accum_k1".to_string(), accum_src(1)));
    v.push(("accum_k7".to_string(), accum_src(7)));
    v.push(("accum_k1000".to_string(), accum_src(1000)));
    v
}

/// Loop-carried accumulator that changes type every `k` iterations of an outer loop, with a loaded element.
fn accum_src(k: u32) -> String {
    with_prelude(&format!(
        "local t = {{}}
         for i = 1, 30 do t[i] = i end
         local function f(n)
           local acc = 0
           for r = 1, n do
             for i = 1, 30 do
               local x = t[i]
               if (r // {k}) % 2 == 1 then x = x + 0.5 end
               acc = acc + x
             end
           end
           return acc
         end
         return H(f(60)) .. ',' .. H(f(7)) .. ',' .. H(f(60))"
    ))
}

// ---------------------------------------------------------------------------------------------------- tests

#[test]
fn policy_programs_match_the_interpreter() {
    for (name, src) in programs() {
        eprintln!("policy program {name}");
        twin(&src);
    }
}

#[test]
fn flip_flop_live_ins_are_retargeted_then_widened_and_settle() {
    for (name, k) in [("flip1", 1), ("flip7", 7), ("flip1000", 1000)] {
        let (_, s) = run_with(&flip_src(k), Mode::Always, cfg("+exithot=2"), None);
        if rn_core::jit_layout::LUA54 {
            continue;
        }
        assert!(s.t15_regions >= 1, "{name}: {s:?}");
        assert!(s.t15_stub_events >= 1, "{name}: {s:?}");
        assert!(
            s.t15_respec >= 1 && s.t15_respec <= 6 && s.t15_giveups == 0,
            "{name}: {s:?}"
        );
        // bounded: two nests (the callee's loop, and since T6 the caller's loop whose call result is speculated), each
        // promoted once and respecialised at most GEN_CAP times
        assert!(s.t15_recompiled <= 2 + 6, "{name}: {s:?}");
    }
}

#[test]
fn while_true_break_loops_get_regions() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    // +force: the profitability gate (upvalue loads are fallbacks) would refuse this nest without it
    let (_, s) = run_with(WHILE_TRUE_SIFT, Mode::Always, cfg("+exithot=2"), None);
    assert!(s.t15_regions >= 1, "{s:?}");
}

#[test]
fn a_one_time_type_change_is_retargeted_once() {
    let (_, s) = run_with(&with_prelude(ONCE), Mode::Always, cfg("+exithot=2"), None);
    if rn_core::jit_layout::LUA54 {
        return;
    }
    assert!(s.t15_respec >= 1 && s.t15_giveups == 0, "{s:?}");
}

#[test]
fn peeled_slots_do_not_count_as_failures() {
    for o in ["+exithot=1", "+exithot=4"] {
        let (_, s) = run_with(&with_prelude(PEEL), Mode::Always, cfg(o), None);
        if rn_core::jit_layout::LUA54 {
            return;
        }
        assert!(s.t15_regions >= 1, "{s:?}");
        // one decision (retarget to the peel form), then silence: 600 entries do not fail any more
        assert_eq!(s.t15_respec, 1, "`{o}`: {s:?}");
        assert_eq!(s.t15_giveups, 0, "`{o}`: {s:?}");
        assert!(s.t15_stub_events <= 2, "`{o}`: {s:?}");
    }
}

#[test]
fn a_benign_failure_rate_is_penalised_not_acted_on() {
    let (_, s) = run_with(&rare_src(), Mode::Always, cfg("+exithot=8"), None);
    if rn_core::jit_layout::LUA54 {
        return;
    }
    assert!(s.t15_regions >= 1, "{s:?}");
    assert!(s.t15_stub_events >= 1 && s.t15_penalties >= 1, "{s:?}");
    assert_eq!(s.t15_respec, 0, "{s:?}");
}

#[test]
fn a_nest_at_the_respecialisation_cap_is_given_up() {
    let (_, s) = run_with(&phases_src(), Mode::Always, cfg("+exithot=2"), None);
    if rn_core::jit_layout::LUA54 {
        return;
    }
    assert!(s.t15_regions >= 1, "{s:?}");
    assert!(s.t15_respec >= 1, "{s:?}");
    assert_eq!(s.t15_giveups, 1, "{s:?}");
    assert_eq!(s.t15_off, 1, "{s:?}");
}

#[test]
fn compiles_per_prototype_are_bounded() {
    let (_, s) = run_with(&many_versions_src(), Mode::Always, cfg("+exithot=1"), None);
    if rn_core::jit_layout::LUA54 {
        return;
    }
    // 6 loops of one prototype: at most tier 1 + 5 recompiles + the give-up reserve (the helper functions below the
    // main chunk add their own tier-1 compiles, which are not recompiles)
    assert!(
        s.t15_recompiled <= 2 * (5 + rn_jit::t15::runtime::GIVEUP_RESERVE as u64),
        "{s:?}"
    );
}

#[test]
fn table_and_key_flips_use_the_run_time_tag_checks() {
    let (_, s) = run_with(TABLE_AND_KEY_FLIPS, Mode::Always, cfg("+exithot=2"), None);
    if rn_core::jit_layout::LUA54 {
        return;
    }
    assert!(s.t15_regions >= 1 && s.t15_respec >= 1, "{s:?}");
}

#[test]
fn respecialising_off_means_no_policy_activity() {
    for (_, src) in programs() {
        let (_, s) = run_with(&src, Mode::Always, cfg("-respec,+exithot=1"), None);
        assert_eq!(
            (
                s.t15_stub_events,
                s.t15_respec,
                s.t15_giveups,
                s.t15_penalties
            ),
            (0, 0, 0, 0),
            "{s:?}"
        );
    }
}

/// C1: respecialised versions execute exactly the bytecodes the interpreter does.
#[test]
fn instruction_counts_are_identical_across_respecialised_versions() {
    for (name, src) in programs() {
        for b in [None, Some(5), Some(37)] {
            let (out0, s0) = run_with(&src, Mode::Always, off(), b);
            for o in [
                "+exithot=2",
                "+exithot=2,-ra",
                "+exithot=2,+randra=3",
                "+exithot=2,+flushall",
                "+exithot=2,+verify",
            ] {
                let (out1, s1) = run_with(&src, Mode::Always, cfg(o), b);
                assert_eq!(out0, out1, "{name}: output, opt `{o}`, budget {b:?}");
                assert_eq!(
                    s0.insns, s1.insns,
                    "{name}: instruction count differs ({} regions, {} respecialisations), opt `{o}`, budget {b:?}",
                    s1.t15_regions, s1.t15_respec
                );
            }
        }
    }
}

/// Lifetime: versions are retired while activations are suspended in older ones; in `Verify` mode freed chunks are
/// inaccessible, so a return into freed code faults.
#[test]
fn versions_retired_while_activations_are_suspended() {
    let src = retire_src();
    let (want, _) = run_with(&src, Mode::Off, off(), None);
    for mode in [Mode::Always, Mode::Verify] {
        for o in ["+exithot=1", "+exithot=2,+randra=4"] {
            let (got, s) = run_with(&src, mode, cfg(o), None);
            assert_eq!(want, got, "{mode:?} `{o}`");
            if !rn_core::jit_layout::LUA54 {
                assert!(s.t15_respec >= 1, "{mode:?} `{o}`: {s:?}");
            }
        }
    }
}

/// Lifetime: prototypes freed while holding retired versions, repeatedly (closures created by `load`).
#[test]
fn prototypes_are_freed_with_live_versions() {
    let prog = "
        local acc = 0
        for round = 1, 40 do
          local f = load('local function h(n, v) local s = v for i = 1, n do s = s + i end return s end ' ..
                         'local r = 0 for c = 1, 60 do r = r + h(10, (c // 3 % 2 == 0) and 1 or 0.5) end return r')
          acc = acc + f()
          f = nil
          if round % 5 == 0 then collectgarbage() end
        end
        return acc";
    let (want, _) = run_with(prog, Mode::Off, off(), None);
    for mode in [Mode::Always, Mode::Verify] {
        let (got, s) = run_with(prog, mode, cfg("+exithot=2"), None);
        assert_eq!(want, got, "{mode:?}");
        if !rn_core::jit_layout::LUA54 {
            assert!(s.t15_respec >= 10, "{mode:?}: {s:?}");
            assert!(s.freed >= 10, "{mode:?}: {s:?}");
        }
    }
}

/// Lifetime: the code cap is hit while the policy wants to recompile: the running version stays, results are right.
#[test]
fn code_cap_pressure_during_respecialisation() {
    let prog = flip_src(1);
    let (want, _) = run_with(&prog, Mode::Off, off(), None);
    let mut st = new_state(Mode::Always, cfg("+exithot=2"));
    // compile everything once, then cap the arena to what is mapped: no new version fits
    assert_eq!(exec(&mut st, "return 1"), "1");
    let s = rn_jit::stats(&mut st);
    rn_jit::set_code_cap(&mut st, s.mapped as usize);
    assert_eq!(exec(&mut st, &prog), want);
    rn_jit::set_code_cap(&mut st, usize::MAX / 2);
    assert_eq!(exec(&mut st, &prog), want);
}

/// `pq`: the versions of the function that runs the flip-flop loop grow and are all kept until it is freed.
#[test]
fn retired_blocks_accumulate_up_to_the_cap() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    let mut st = new_state(Mode::Always, cfg("+exithot=1"));
    let prog = with_prelude(
        "local function h(n, v)
           local s = v
           for i = 1, n do s = s + i end
           return s, pq(0), pq(1), pq(3)
         end
         local r, cur, ret, offs = 0, 0, 0, 0
         for c = 1, 200 do
           local a, b, d, e = h(20, (c // 2 % 2 == 0) and 1 or 0.5)
           r = r + a; cur = b; ret = d; offs = e
         end
         return H(r) .. ' ' .. cur .. ' ' .. ret .. ' ' .. offs",
    );
    let out = exec(&mut st, &prog);
    let parts: Vec<&str> = out.split(' ').collect();
    let (cur, ret): (i64, i64) = (parts[1].parse().unwrap(), parts[2].parse().unwrap());
    assert!((1..=7).contains(&cur), "{out}");
    assert_eq!(ret, cur, "every older version stays retired: {out}");
}

/// Review fix 1: a countdown that fired in a *stale* version's pad is re-armed, and the current version's failures
/// reach the policy afterwards.
#[test]
fn a_stale_fire_does_not_kill_the_countdown() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    let mut st = new_state(Mode::Always, cfg("+exithot=4"));
    // promote the nest (version 1), then simulate: the countdown fired below zero in version 0's pad
    let prog = with_prelude(
        "local function h(n, v)
           local s = v
           for i = 1, n do s = s + i end
           local c0 = pq(0)
           pset(-1)
           pev(0, 0, 0)
           return c0, pq(4)
         end
         local c0, f
         for c = 1, 20 do c0, f = h(10, 1) end
         -- the current version's failures still reach the policy: flip the live-in type until it respecialises
         local r = 0
         for c = 1, 300 do r = r + h(30, (c // 3 % 2 == 0) and 1 or 0.5) end
         return c0 .. ' ' .. f",
    );
    let out = exec(&mut st, &prog);
    let parts: Vec<&str> = out.split(' ').collect();
    assert!(parts[0].parse::<i64>().unwrap() >= 1, "{out}");
    assert!(parts[1].parse::<i64>().unwrap() >= 0, "re-armed: {out}");
    let s = rn_jit::stats(&mut st);
    assert!(s.t15_stale_events >= 1 && s.t15_respec >= 1, "{s:?}");
}

/// Review fix 2: the normal compile cap is spent (six loops promoted, then they flip): the failing regions are removed
/// with the reserved slots instead of being kept for good.
#[test]
fn an_exhausted_compile_cap_still_gives_the_nest_up() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    let (_, s) = run_with(&many_versions_src(), Mode::Always, cfg("+exithot=1"), None);
    assert!(s.t15_giveups >= 1, "{s:?}");
    assert!(
        s.t15_recompiled <= 2 * (5 + rn_jit::t15::runtime::GIVEUP_RESERVE as u64),
        "{s:?}"
    );
}

/// Review fix 3: a guard that feedback cannot change (`#t` whose `__len` flips between int and float) is given up at the
/// first decision instead of recompiling the same speculation three times.
#[test]
fn unchangeable_guards_are_given_up_at_once() {
    let src = with_prelude(
        "local function h(n, t)
           local s = 0
           for i = 1, n do s = s + #t end
           return s
         end
         local flip = false
         local t = setmetatable({}, {__len = function() flip = not flip; return flip and 3 or 2.5 end})
         local u = setmetatable({}, {__len = function() return 4 end})
         local r = {}
         for c = 1, 200 do r[#r + 1] = H(h(12, (c % 2 == 0) and t or u)) end
         return table.concat(r, ',')",
    );
    let (want, _) = run_with(&src, Mode::Off, off(), None);
    let (got, s) = run_with(&src, Mode::Always, cfg("+exithot=2"), None);
    assert_eq!(want, got);
    if !rn_core::jit_layout::LUA54 {
        assert!(s.t15_respec <= 2, "{s:?}");
    }
}

/// Review fix 4: heapsort's `sift` shape without `+force`: the gate must refuse it (upvalue fallbacks weigh double).
#[test]
fn the_gate_refuses_the_upvalue_heavy_sift() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    let src = "
        local a = {9, 4, 7, 1, 8, 2, 6, 3, 5, 15, 12, 11, 14, 10, 13}
        local function sift(lo, hi)
          local root = lo
          while true do
            local child = root * 2
            if child > hi then break end
            if child < hi and a[child] < a[child + 1] then child = child + 1 end
            if a[root] < a[child] then a[root], a[child] = a[child], a[root]; root = child else break end
          end
        end
        local function drive(n) for r = 1, n do a[1] = 0; sift(1, 15) end return a[1] end
        return drive(300) + drive(300)";
    let mut c = T15Config::for_mode(T15Mode::On);
    c.hot = 2;
    // (`-hoist`: since T6 `GETUPVAL` is inlined unless this switch is given; the gate weight is for the fallback form)
    c.opt = OptFlags::parse("-hoist");
    let (_, s) = run_with(src, Mode::Always, c, None);
    let sift_refused = s.t15_regions == 0;
    assert!(sift_refused, "{s:?}");
}

/// Review fix 4: an outer activation still runs in a *retired* block when an inner call respecialises the nest: the
/// callee changes the outer's local with `debug.setlocal` after the inner calls respecialised, so the outer's
/// stub fails in the old version's pad (`t15_stale_events` counts those fires). In `Verify` mode a freed block would fault.
#[test]
fn outer_activations_really_run_in_retired_blocks() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    let src = with_prelude(
        "local function h(depth, v)
           local k = 7
           local s = v
           for i = 1, 30 do
             s = s + i + k
             if i == 3 and depth > 0 then
               for k = 1, 6 do pcall(h, depth - 1, (k % 2 == 0) and 1 or 0.5) end
               pset(-1)
               pcall(function() debug.setlocal(3, 3, 0.25) end)
             end
           end
           return s
         end
         local r = {}
         for c = 1, 12 do r[#r + 1] = H(h(2, 1)) end
         return table.concat(r, ',')",
    );
    let (want, _) = run_with(&src, Mode::Off, off(), None);
    for mode in [Mode::Always, Mode::Verify] {
        let (got, s) = run_with(&src, mode, cfg("+exithot=1"), None);
        assert_eq!(want, got, "{mode:?}");
        assert!(s.t15_respec >= 1, "{mode:?}: {s:?}");
        assert!(
            s.t15_stale_events >= 1,
            "{mode:?}: no code ran in a retired block: {s:?}"
        );
    }
}

/// queens' shape: boolean marks flipping between `false` and `true` at several loads of one nest. One decision widens
/// all of them; without the shortcut every load costs a respecialisation and the nest is given up.
#[test]
fn boolean_flips_at_several_loads_settle_in_one_decision() {
    let src = "
        local N, count = 12, 0
        local cols, d1, d2 = {}, {}, {}
        local function place(r)
          if r > N then count = count + 1; return end
          for c = 1, N do
            if not cols[c] and not d1[r + c] and not d2[r - c + N] then
              cols[c], d1[r + c], d2[r - c + N] = true, true, true
              place(r + 1)
              cols[c], d1[r + c], d2[r - c + N] = false, false, false
            end
          end
        end
        place(1)
        return count";
    let (want, _) = run_with(src, Mode::Off, off(), None);
    for c in [cfg("+exithot=4"), cfg_on("+exithot=2", 4), cfg_on("", 4096)] {
        let (got, s) = run_with(src, Mode::Always, c, None);
        assert_eq!(want, got);
        if !rn_core::jit_layout::LUA54 {
            assert_eq!(s.t15_giveups, 0, "{s:?}");
            assert!(s.t15_respec <= 2, "{s:?}");
        }
    }
}

/// T6/T7 interaction: an inline `math` intrinsic whose callee is replaced mid-run. The identity guard then fails at every
/// call (slow path), or the replacement returns another type (the stub after the call fails on the speculated result).
/// Either way the policy demotes the site to an ordinary `CALL` (`t15_demotions`) instead of failing forever or burning
/// all respecialisations.
#[test]
fn failing_intrinsic_sites_are_demoted_to_ordinary_calls() {
    for (name, repl) in [
        ("same type", "function(x) return x * 2.0 end"),
        ("other type", "function(x) return 3 end"),
        ("string", "function(x) return 'a' end"),
    ] {
        let src = with_prelude(&format!(
            "local function f(n)
               local s = 0.0
               for i = 1, n do s = s + math.sqrt(i) end
               return s
             end
             local r = {{}}
             for c = 1, 20 do r[#r + 1] = H(f(40)) end
             math.sqrt = {repl}
             for c = 1, 400 do r[#r + 1] = tostring(select(2, pcall(f, 40))) end
             return table.concat(r, ',')"
        ));
        let (want, _) = run_with(&src, Mode::Off, off(), None);
        for o in [
            "+exithot=2",
            "+exithot=3,-ra",
            "+exithot=2,+verify,+flushall",
        ] {
            let (got, s) = run_with(&src, Mode::Always, cfg(o), None);
            assert_eq!(want, got, "{name} `{o}`");
            if !rn_core::jit_layout::LUA54 {
                assert!(s.t15_demotions >= 1, "{name} `{o}`: {s:?}");
                assert!(s.t15_giveups == 0, "{name} `{o}`: {s:?}");
            }
        }
    }
}

// ------------------------------------------------------------------------------------------ T6 review fixes

/// Warm program for the demotion tests: a compiled loop with an inline `math.sqrt` / `math.floor`.
const HG_PRELUDE: &str = "
    function H(x)
      if math.type(x) == 'integer' then return 'i' .. x end
      if math.type(x) ~= 'float' then return tostring(x) end
      return string.format('%016x', string.unpack('<i8', string.pack('<d', x)))
    end
";

const SQRT_SRC: &str = "
    function sqf(n)
      local s = 0.0
      for i = 1, n do s = s + math.sqrt(i) end
      return s
    end
    function flf(n, x)
      local s = 0
      for i = 1, n do s = s + math.floor(x) end
      return s
    end";

/// Headroom of the memory limit in the refused-recompile tests: the loops allocate nothing, but a recompile is charged
/// whole pages.
const TIGHT: usize = 4096;

fn limit_now(st: &mut LuaState, head: usize) {
    let mem = st.heap().mem_mut();
    let now = mem.total_bytes() + mem.ext_bytes();
    mem.set_limit(now + head);
}

/// Failing stage: `math.sqrt` replaced, numeric result only (nothing is allocated, so the memory limit does not bother
/// the interpreter).
const FAIL_STAGE: &str = "
    math.sqrt = function(x) return x * 2.0 end
    local t = 0.0
    for c = 1, 400 do t = t + sqf(40) end
    return t";

/// The "stay" path of the intrinsic failure pad: the policy helper clobbers `r11` and returns 0 (the recompile is
/// refused by the memory limit); the budget and the instruction counts must be those of tier 1 (reviewer findings 1, 4).
#[test]
fn the_stay_path_of_the_failure_pad_keeps_the_budget() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    let warm = format!("{HG_PRELUDE}{SQRT_SRC} return H(sqf(40)) .. H(sqf(40))");
    for b in [None, Some(100000), Some(37)] {
        let mut st0 = new_state(Mode::Always, off());
        if let Some(bb) = b {
            rn_jit::set_count_budget(&mut st0, bb);
        }
        let mut o0 = exec(&mut st0, &warm);
        limit_now(&mut st0, TIGHT);
        o0 += &exec(&mut st0, FAIL_STAGE);
        let s0 = rn_jit::stats(&mut st0);
        for o in ["+exithot=2", "+exithot=2,-ra", "+exithot=3,+verify"] {
            let mut st = new_state(Mode::Always, cfg(o));
            if let Some(bb) = b {
                rn_jit::set_count_budget(&mut st, bb);
            }
            let mut o1 = exec(&mut st, &warm);
            limit_now(&mut st, TIGHT);
            o1 += &exec(&mut st, FAIL_STAGE);
            let s1 = rn_jit::stats(&mut st);
            assert_eq!(o0, o1, "output `{o}` budget {b:?}");
            if b != Some(37) {
                assert!(s1.t15_stub_events >= 1, "the pad never fired: {s1:?}");
            }
            assert_eq!(
                s0.insns, s1.insns,
                "instruction count `{o}` budget {b:?}: {s1:?}"
            );
        }
    }
}

/// A demotion whose recompile is refused is rolled back: the site stays demotable, and once there is room the next
/// decision demotes it (no give-up; reviewer finding 3).
#[test]
fn a_refused_demotion_is_retried_not_given_up() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    let mut st = new_state(Mode::Always, cfg("+exithot=2"));
    exec(
        &mut st,
        &format!("{HG_PRELUDE}{SQRT_SRC} return H(sqf(40))"),
    );
    limit_now(&mut st, TIGHT);
    exec(&mut st, FAIL_STAGE);
    let mid = rn_jit::stats(&mut st);
    assert!(mid.t15_stub_events >= 1 && mid.t15_respec == 0, "{mid:?}");
    assert_eq!(mid.t15_giveups, 0, "{mid:?}");
    st.heap().mem_mut().set_limit(usize::MAX);
    for _ in 0..6 {
        exec(
            &mut st,
            "local t = 0.0 for c = 1, 400 do t = t + sqf(40) end return t",
        );
        if rn_jit::stats(&mut st).t15_respec > 0 {
            break;
        }
    }
    let end = rn_jit::stats(&mut st);
    assert_eq!(end.t15_giveups, 0, "{end:?}");
    assert!(end.t15_respec > mid.t15_respec, "{mid:?} -> {end:?}");
}

/// Only identity / stack / `CallInfo` failures feed the demotion countdown: a hook installed inside the loop makes the
/// next inline call take the slow path (the tier-1 `CALL` leaves for the interpreter) without counting (reviewer finding 2).
#[test]
fn hook_misses_of_inline_calls_do_not_demote() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    // a coroutine suspended in the loop right before the inline call; the hook is set on it from outside, so the call
    // that follows the resume meets `hookmask != 0` (no back-edge poll in between)
    let src = format!(
        "{HG_PRELUDE}
         function body(n)
           local s = 0.0
           for i = 1, n do
             coroutine.yield(s)
             s = s + math.sqrt(i)
           end
           return s
         end
         local r = {{}}
         for c = 1, 300 do
           local co = coroutine.create(body)
           coroutine.resume(co, 12)
           coroutine.resume(co)
           coroutine.resume(co)
           debug.sethook(co, function() end, '', 1000000)
           local ok, v = coroutine.resume(co)
           while coroutine.status(co) ~= 'dead' do ok, v = coroutine.resume(co) end
           r[#r + 1] = H(v)
         end
         return #r .. r[1] .. r[300]"
    );
    let (want, _) = run_with(&src, Mode::Off, off(), None);
    let (got, s) = run_with(&src, Mode::Always, cfg("+exithot=2"), None);
    assert_eq!(want, got);
    assert_eq!(s.t15_demotions, 0, "{s:?}");
    assert_eq!(s.t15_stub_events, 0, "{s:?}");
}

// ------------------------------------------------------------------------------------------------ hooks

/// Establishes (and pins down) the interaction of regions with hooks: compiled code is entered only when no hook is
/// installed. A function called under a count or line hook runs in the interpreter, so region code never runs under
/// a hook; installing the hook inside a running loop leaves compiled code at the next back-edge poll.
#[test]
fn regions_do_not_run_while_a_hook_is_installed() {
    let prog = "
        local function h(n)
          local s = 0
          for i = 1, n do s = s + i end
          return s
        end
        local r = 0
        for c = 1, 50 do r = r + h(100) end
        return r";
    // (the hook is installed by an earlier chunk: the chunk that runs the loop is not itself entered compiled)
    let run = |hook: &str| -> (String, Stats, Stats) {
        let mut st = new_state(Mode::Always, cfg("+exithot=2"));
        if !hook.is_empty() {
            assert_eq!(exec(&mut st, hook), "1");
        }
        let before = rn_jit::stats(&mut st);
        let out = exec(&mut st, prog);
        (out, before, rn_jit::stats(&mut st))
    };
    let (o_none, b0, a0) = run("");
    let (o_count, b1, a1) = run("debug.sethook(function() end, '', 100) return 1");
    let (o_line, b2, a2) = run("debug.sethook(function() end, 'l') return 1");
    let (o_call, b3, a3) = run("debug.sethook(function() end, 'cr') return 1");
    for o in [&o_none, &o_count, &o_line, &o_call] {
        assert_eq!(o, "252500");
    }
    assert!(a0.enters > b0.enters, "no hook: compiled code runs");
    if !rn_core::jit_layout::LUA54 {
        assert!(a0.t15_regions >= 1, "no hook: a region exists: {a0:?}");
    }
    for (what, b, a) in [("count", b1, a1), ("line", b2, a2), ("call", b3, a3)] {
        assert_eq!(
            a.enters, b.enters,
            "{what} hook: nothing compiled is entered"
        );
        assert_eq!(a.osr_enters, b.osr_enters, "{what} hook: no OSR entry");
        assert_eq!(
            a.t15_regions, b.t15_regions,
            "{what} hook: no region is built"
        );
    }
    // 4. the hook is installed in the middle of a hot loop by a callee: compiled code is left at the next poll and
    //    the remaining iterations run in the interpreter with exact hook counts
    let mid = "
        local hits = 0
        local function install() debug.sethook(function() hits = hits + 1 end, '', 10) end
        local function h(n)
          local s = 0
          for i = 1, n do
            s = s + i
            if i == 500 then install() end
          end
          debug.sethook()
          return s
        end
        local r = 0
        for c = 1, 30 do r = r + h(1000) end
        return r .. ' ' .. hits";
    let (want, _) = run_with(mid, Mode::Off, off(), None);
    let (got, s) = run_with(mid, Mode::Always, cfg("+exithot=2"), None);
    assert_eq!(want, got);
    assert!(s.enters >= 1, "{s:?}");
}

// ------------------------------------------------------------------------------------------------ mutants

#[cfg(feature = "t15_mutants")]
mod mutants {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    /// Each must make the rest of this binary fail.
    const BUGS: &[&str] = &[
        "pol_no_widen",
        "pol_no_peel_exempt",
        "pol_no_penalty",
        "pol_no_giveup",
        "pol_no_fail_count",
        "pol_drop_old_block",
        "pol_stale_no_rearm",
        "pol_no_demote",
        "pol_no_budget_reload_stay",
        "pol_demote_no_rollback",
        "pol_no_bool_widen",
        "pol_cap_no_giveup",
        "gate_upval_single",
        // the table-path mutants that only a respecialised (widened) version reaches
        "tbl_no_table_tag",
        "tbl_no_key_tag",
        "tbl_widen_unchecked",
    ];

    fn child(mutant: Option<&str>) -> (Option<bool>, String) {
        let exe = std::env::current_exe().expect("exe");
        let mut cmd = Command::new(exe);
        cmd.args([
            "--skip",
            "every_policy_mutant_is_detected",
            "--test-threads=2",
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
                None if t0.elapsed() > Duration::from_secs(120) => {
                    let _ = c.kill();
                    let _ = c.wait();
                    return (None, String::new());
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        }
    }

    #[test]
    fn every_policy_mutant_is_detected() {
        if rn_core::jit_layout::LUA54 {
            return;
        }
        let (ok, text) = child(None);
        assert_eq!(ok, Some(true), "the unmutated suite must pass:\n{text}");
        let mut undetected = Vec::new();
        for m in BUGS {
            let (ok, _) = child(Some(m));
            eprintln!(
                "mutant {m}: {}",
                if ok == Some(true) {
                    "SURVIVED"
                } else {
                    "detected"
                }
            );
            if ok == Some(true) {
                undetected.push(*m);
            }
        }
        assert!(
            undetected.is_empty(),
            "mutants NOT detected: {undetected:?}"
        );
    }
}
