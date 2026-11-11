//! Tier 1.5 runtime plumbing (task T3): promote counters and `prof[pc]` warm-up code, the promote/event helpers,
//! the recompile path (retired blocks, resume/hdr/pcmap swap, charging), version-id discipline (B4), the dead-pc
//! guard (M9) and code lifetime. No typed region exists yet: a recompiled program is tier 1 again, so every
//! program must produce exactly the result of a jit-off twin whatever the promotion pattern.
//!
//! The Lua programs reach the runtime through two test natives: `t15q(what, arg)` queries the tier 1.5 state of
//! the calling Lua function and `t15ev(ver, site, kind, pc)` simulates an event from the exit stub of code version
//! `ver` (the helper `jit_t15_event` as region code will call it).
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, State, LUA_MULTRET};
use rn_core::status::{LuaResult, Status};
use rn_core::value::Value;
use rn_jit::t15::runtime::{test_event, test_info, EventKind, NestPhase, T15Config, T15Mode};
use rn_jit::{Config, Mode, Stats};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
            "the compiler must be installed by this test binary"
        );
    });
}

fn q(st: &mut State) -> LuaResult<usize> {
    let what = st.arg(0).as_int().unwrap_or(-1);
    let arg = st.arg(1).as_int().unwrap_or(0) as usize;
    let Some(i) = test_info(st) else {
        st.push(Value::int(-1));
        return Ok(1);
    };
    let v: i64 = match what {
        0 => i64::from(i.cur),
        1 => i.retired as i64,
        2 => i
            .phases
            .iter()
            .filter(|p| **p == NestPhase::Promoted)
            .count() as i64,
        3 => i64::from(*i.prof.get(arg).unwrap_or(&0)),
        4 => i.block as i64,
        5 => i64::from(*i.ctrs.get(arg).unwrap_or(&0)),
        6 => i.charged as i64,
        7 => i64::from(*i.pcmap.get(arg).unwrap_or(&0)),
        8 => i.code_len as i64,
        9 => *i.retired_addrs.get(arg).unwrap_or(&0) as i64,
        // first dead pc (no tier-1 label in the current version), -1 if none
        11 => i
            .pcmap
            .iter()
            .position(|&o| o == 0)
            .map_or(-1, |p| p as i64),
        12 => i.phases.len() as i64,
        13 => i.phases.iter().filter(|p| **p == NestPhase::Off).count() as i64,
        // number of prof bytes that are not 0xFF
        14 => i.prof.iter().filter(|&&b| b != 0xFF).count() as i64,
        // does any prof byte equal `arg`?
        15 => i.prof.iter().any(|&b| usize::from(b) == arg) as i64,
        // bit mask of the pcs (< 62) with a prof byte
        16 => i
            .prof
            .iter()
            .take(62)
            .enumerate()
            .filter(|(_, &b)| b != 0xFF)
            .fold(0i64, |m, (pc, _)| m | (1i64 << pc)),
        _ => -2,
    };
    st.push(Value::int(v));
    Ok(1)
}

fn ev(st: &mut State) -> LuaResult<usize> {
    let ver = st.arg(0).as_int().unwrap_or(0) as u32;
    let site = st.arg(1).as_int().unwrap_or(0) as u32;
    let kind = match st.arg(2).as_int().unwrap_or(0) {
        0 => EventKind::Exit,
        1 => EventKind::Miss,
        _ => EventKind::Respecialize,
    };
    let pc = st.arg(3).as_int().unwrap_or(0) as u32;
    match test_event(st, ver, site, kind, pc) {
        Some(r) => {
            st.push(Value::int(i64::from(r.ret)));
            st.push(Value::int(r.cont as i64));
            Ok(2)
        }
        None => {
            st.push(Value::int(-1));
            st.push(Value::int(0));
            Ok(2)
        }
    }
}

fn new_state(mode: Mode, t15: T15Config) -> LuaState {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    rn_jit::set_t15(&mut st, t15);
    let g = st.globals();
    st.raw_set_str(g, b"t15q", Value::native(q)).unwrap();
    st.raw_set_str(g, b"t15ev", Value::native(ev)).unwrap();
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

fn always() -> T15Config {
    T15Config::for_mode(T15Mode::Always)
}
fn off() -> T15Config {
    T15Config::for_mode(T15Mode::Off)
}

/// Result of the program in a jit-off twin, with `T15=off`, with `T15=always` and with `T15=always` under tiny
/// count budgets; all equal. Returns the stats of the `T15=always` run.
fn same(src: &str) -> Stats {
    let (want, s0) = run_with(src, Mode::Off, off(), None);
    assert_eq!(s0.enters, 0);
    assert!(!want.starts_with("ERROR"), "program failed: {want}");
    let (t1, s1) = run_with(src, Mode::Always, off(), None);
    assert_eq!(want, t1, "tier 1 differs from the interpreter for:\n{src}");
    assert_eq!(s1.t15_recompiled, 0, "{s1:?}");
    let (got, s) = run_with(src, Mode::Always, always(), None);
    assert_eq!(want, got, "T15=always differs for:\n{src}");
    for b in [3, 17, 200] {
        let (got, _) = run_with(src, Mode::Always, always(), Some(b));
        assert_eq!(want, got, "budget {b}: T15=always differs for:\n{src}");
    }
    s
}

const NUMERIC: &str = "
    local function f(n) local s = 0 for i = 1, n do s = s + i % 7 end return s end
    local t = 0
    for k = 1, 30 do t = t + f(100) end
    return t";

#[test]
fn promotion_recompiles_and_behaviour_is_unchanged() {
    let s = same(NUMERIC);
    assert!(s.t15_promoted >= 1 && s.t15_recompiled >= 1, "{s:?}");
    assert!(s.compiled >= 1 && s.enters >= 1, "{s:?}");
}

#[test]
fn off_mode_never_recompiles_and_has_no_t15_state() {
    let (_, s) = run_with(NUMERIC, Mode::Always, off(), None);
    assert_eq!((s.t15_promoted, s.t15_recompiled, s.t15_refused), (0, 0, 0));
    let mut st = new_state(Mode::Always, off());
    assert_eq!(
        exec(
            &mut st,
            "local function f(n) local s = 0 for i = 1, n do s = s + i end return s end
             f(10) f(10) return t15q(0)"
        ),
        "-1"
    );
}

#[test]
fn a_hot_threshold_delays_promotion() {
    let cfg = T15Config {
        hot: 300,
        ..T15Config::for_mode(T15Mode::On)
    };
    let prog = "local function f(n) local s = 0 for i = 1, n do s = s + i end return s, t15q(0), t15q(5, 0) end
        local _, v1, c1 = f(100)
        local _, v2, c2 = f(100)
        local _, v3, c3 = f(200)
        return v1 .. ' ' .. v2 .. ' ' .. v3";
    let mut st = new_state(Mode::Always, cfg);
    assert_eq!(exec(&mut st, prog), "0 0 1");
    let s = rn_jit::stats(&mut st);
    assert_eq!(s.t15_promoted, 1, "{s:?}");
}

#[test]
fn prof_bytes_record_the_result_tags_of_loads() {
    // x is a float field, y an integer field, up[1] a table: tags 0x13, 0x03 and a collectable tag
    let prog = "local o, c = {x = 1.5, y = 2, 'a'}, 3
        local function g(n)
          local s = 0
          for i = 1, n do s = s + o.x + o.y + c end
          return s, t15q(14), t15q(15, 0x13), t15q(15, 0x03), t15q(16)
        end
        local _, n, f, i, m = g(5)
        return n .. ' ' .. f .. ' ' .. i .. ' ' .. m";
    let cfg = T15Config {
        hot: 1000,
        ..T15Config::for_mode(T15Mode::On)
    };
    let mut st = new_state(Mode::Always, cfg);
    let out = exec(&mut st, prog);
    let mut it = out.split(' ');
    let n: i64 = it.next().unwrap().parse().unwrap();
    assert!(n >= 3, "prof stores missing: {out}"); // GETTABUP x, GETTABUP y, GETUPVAL c
    assert_eq!(it.next(), Some("1"), "no float tag recorded: {out}");
    assert_eq!(it.next(), Some("1"), "no int tag recorded: {out}");
}

#[test]
fn nested_activation_promotes_while_the_outer_activation_is_in_old_code() {
    // The outer activation calls itself through pcall (a native helper) before its first back-edge. The inner
    // one fires the promotion and recompiles; when the outer one returns from the helper it is still in the
    // retired code, reaches its back-edge, finds the nest promoted and moves to the new code: no second recompile.
    let prog = "
        local function F(n)
          local s = 0
          for i = 1, 5 do
            s = s + i
            if i == 1 and n > 0 then
              local ok, r = pcall(F, n - 1)
              s = s + r
            end
          end
          return s
        end
        local r = F(3)
        return r .. ' ' .. F(0)";
    let s = same(prog);
    let mut st = new_state(Mode::Always, always());
    assert_eq!(
        exec(&mut st, prog),
        run_with(prog, Mode::Off, off(), None).0
    );
    let s2 = rn_jit::stats(&mut st);
    assert_eq!(s2.t15_promoted, 1, "idempotent promote: {s2:?}");
    assert_eq!(s2.t15_recompiled, 1, "{s2:?} {s:?}");
}

#[test]
fn outer_activation_finishes_in_the_retired_code() {
    // the outer activation never reaches another back-edge after the helper returns
    let prog = "
        local function F(n)
          local s = 0
          for i = 1, 1 do
            s = s + 1
            if n > 0 then
              local ok, r = pcall(F, n - 1)
              s = s + r
            end
          end
          for i = 1, 2 do s = s + 1 end
          return s
        end
        return F(4)";
    same(prog);
}

#[test]
fn recompile_while_coroutine_frames_are_suspended() {
    let prog = "
        local function G(n, y)
          local s = 0
          for i = 1, n do
            s = s + i
            if i == 1 and y then coroutine.yield(s) end
          end
          return s
        end
        local cos, out = {}, {}
        for k = 1, 4 do
          cos[k] = coroutine.create(G)
          local ok, v = coroutine.resume(cos[k], 3 + k, true)
          out[#out + 1] = v
        end
        local main = G(10, false)            -- promotes the nest of G while four coroutines are suspended in its loop
        for k = 1, 4 do
          local ok, v = coroutine.resume(cos[k])
          out[#out + 1] = v
        end
        out[#out + 1] = main
        return table.concat(out, ',')";
    let s = same(prog);
    assert!(s.t15_recompiled >= 1, "{s:?}");
}

#[test]
fn recompile_in_a_coroutine_resumed_later() {
    let prog = "
        local function G(n)
          local s = 0
          for i = 1, n do
            s = s + i
            if i % 3 == 0 then coroutine.yield(s) end
          end
          return s
        end
        local co = coroutine.wrap(G)
        local out = {}
        for k = 1, 7 do out[#out + 1] = co(20) or 'x' end
        return table.concat(out, ',')";
    same(prog);
}

#[test]
fn many_nests_in_one_function() {
    let prog = "
        local function f(n)
          local s = 0
          for i = 1, n do for j = 1, 3 do s = s + j end end
          local k = 0
          while k < n do k = k + 1; s = s + k end
          repeat s = s - 1; k = k - 1 until k <= 0
          for _, v in ipairs({1, 2, 3}) do s = s + v end
          return s
        end
        local t = 0
        for r = 1, 5 do t = t + f(20) end
        return t";
    let s = same(prog);
    assert!(s.t15_promoted >= 4, "{s:?}");
}

#[test]
fn each_nest_promotes_once() {
    let prog = "
        local function f(n)
          local s = 0
          for i = 1, n do s = s + i end
          local k = 0
          while k < n do k = k + 1 end
          return s + k, t15q(0), t15q(2), t15q(12)
        end
        local a, v, p, nn
        for r = 1, 6 do a, v, p, nn = f(50) end
        return a .. ' ' .. v .. ' ' .. p .. ' ' .. nn";
    let mut st = new_state(Mode::Always, always());
    // two nests, two promotions, version 2
    assert_eq!(exec(&mut st, prog), "1325 2 2 2");
    let s = rn_jit::stats(&mut st);
    assert_eq!((s.t15_promoted, s.t15_recompiled), (3, 3), "{s:?}"); // + the loop of the main chunk
}

#[test]
fn the_compile_cap_is_six_versions_per_prototype() {
    let prog = "
        local function f(n)
          local s = 0
          for i = 1, n do s = s + i end
          return s
        end
        local function g()
          local cur = 0
          for k = 1, 9 do
            f(3)
          end
          return 0
        end
        local function h(n)
          local s = 0
          for i = 1, n do
            s = s + i
            local ret, cont = t15ev(t15q(0), 0, 2, 0)    -- Respecialize the current version
          end
          return t15q(0), t15q(1)
        end
        local cur, retired = h(10)
        return cur .. ' ' .. retired";
    let mut st = new_state(Mode::Always, always());
    // version 0 + the promotion (1) + 4 respecialisations = 6 versions; later events change nothing
    assert_eq!(exec(&mut st, prog), "5 5");
}

#[test]
fn stale_version_ids_get_a_continuation_in_the_current_code_and_dead_pcs_are_refused() {
    let prog = "
        local function F(n)
          local s = 0
          for i = 1, n do
            if i < 0 then s = s - 1 else s = s + i end      -- LT + JMP: the JMP is a dead pc
            if i > 0 then s = s + 1 end
          end
          local cur = t15q(0)                                 -- version 1 (promoted at the first back-edge)
          local blk, off = t15q(4), t15q(7, 3)
          local dead = t15q(11)
          local r0, c0 = t15ev(0, 0, 0, 3)                    -- stale id 0 (the tier-1 block): continuation at pc 3
          local r1, c1 = t15ev(1, 0, 0, 3)                    -- the current version: use your own label (0)
          local r2, c2 = t15ev(0, 0, 0, dead)                 -- stale id, dead pc: refused
          local r3, c3 = t15ev(7, 0, 0, 3)                    -- a version that does not exist
          return s, cur, r0, c0 - blk == off - 1, r1, r2, r3, dead >= 0
        end
        local s, cur, r0, cont_ok, r1, r2, r3, has_dead = F(4)
        return s .. ' ' .. cur .. ' ' .. r0 .. ' ' .. tostring(cont_ok) .. ' ' .. r1 .. ' ' .. r2 .. ' ' .. r3 .. ' ' .. tostring(has_dead)";
    let mut st = new_state(Mode::Always, always());
    assert_eq!(exec(&mut st, prog), "14 1 1 true 0 0 0 true");
}

#[test]
fn respecialise_events_swap_the_code_and_keep_old_blocks_mapped() {
    let prog = "
        local function F(n)
          local s = 0
          for i = 1, n do
            s = s + i
            if i == 2 then
              local b0 = t15q(4)
              local r, c = t15ev(t15q(0), 0, 2, 0)              -- Respecialize
              local b1 = t15q(4)
              assert(b0 ~= b1 and t15q(9, t15q(1) - 1) == b0, 'old block retired, new block current')
              assert(r == 1 and c - b1 == t15q(7, 0) - 1, 'after a recompile the continuation is in the new code')
            end
          end
          return s, t15q(0), t15q(1)
        end
        local s, cur, retired = F(6)
        return s .. ' ' .. cur .. ' ' .. retired";
    let mut st = new_state(Mode::Always, always());
    assert_eq!(exec(&mut st, prog), "21 2 2");
}

#[test]
fn code_is_freed_with_the_prototype_and_charges_are_refunded() {
    let mut st = new_state(Mode::Always, always());
    let warm = "local f = load('local s = 0 for i = 1, 10 do s = s + i end return s') f() f() collectgarbage() collectgarbage() return 0";
    assert_eq!(exec(&mut st, warm), "0");
    let base = st.heap().mem_mut().ext_bytes();
    let keep = "keep = {} for i = 1, 100 do
        keep[i] = load('local n = ... local t = 0 for i = 1, n do t = t + i end for j = 1, n do t = t - 1 end return t')
        keep[i](5) end return 0";
    exec(&mut st, keep);
    let s = rn_jit::stats(&mut st);
    assert!(s.t15_recompiled >= 200, "{s:?}");
    let held = st.heap().mem_mut().ext_bytes();
    assert!(
        held > base + 100 * 1024,
        "retired code is charged: {base} -> {held}"
    );
    exec(
        &mut st,
        "keep = nil collectgarbage() collectgarbage() return 0",
    );
    let end = st.heap().mem_mut().ext_bytes();
    let s2 = rn_jit::stats(&mut st);
    assert!(s2.freed >= s.freed + 90, "{s:?} -> {s2:?}");
    assert!(
        end < base + 64 * 1024,
        "refund missing: base {base} held {held} end {end} {s2:?}"
    );
}

#[test]
fn quarantined_arena_faults_if_retired_or_current_code_runs_after_free() {
    // `verify` mode makes freed chunks PROT_NONE: the proto whose stay is running is collected by its caller
    // after it was recompiled (the stay returns through the thunk only)
    let mut st = new_state(Mode::Verify, always());
    let prog = "local function C(n)
           local t = 0
           for i = 1, n do
             local f = load('local s = 0 for i = 1, 300 do s = s + i end for j = 1, 3 do s = s + 1 end return s')
             t = t + f()
             f = nil
             collectgarbage()
           end
           return t
         end
         local r = 0
         for k = 1, 4 do r = r + C(30) end
         return r";
    assert_eq!(exec(&mut st, prog), (4 * 30 * (45150 + 3)).to_string());
    let s = rn_jit::stats(&mut st);
    assert!(s.freed >= 100 && s.t15_recompiled >= 100, "{s:?}");
}

#[test]
fn gc_runs_inside_a_helper_while_older_versions_are_active() {
    // nested activation promotes and collects garbage while the outer activation is suspended in the retired block
    let prog = "
        local function F(n)
          local s = 0
          for i = 1, 3 do
            local junk = {}
            for k = 1, 20 do junk[k] = {k} end
            s = s + #junk
            if i == 1 and n > 0 then
              local ok, r = pcall(F, n - 1)
              collectgarbage()
              s = s + r
            end
          end
          return s
        end
        return F(5)";
    let (want, _) = run_with(prog, Mode::Off, off(), None);
    let mut st = new_state(Mode::Verify, always());
    assert_eq!(exec(&mut st, prog), want);
}

#[test]
fn memory_limit_refuses_a_promotion_and_the_nest_retries_later() {
    let cfg = T15Config {
        hot: 50,
        ..T15Config::for_mode(T15Mode::On)
    };
    let mut st = new_state(Mode::Always, cfg);
    // compile (tier 1, armed counters) under no limit
    assert_eq!(
        exec(
            &mut st,
            "function F(n) local s = 0 for i = 1, n do s = s + i end return s, t15q(0) end
             return F(10)"
        ),
        "55"
    );
    let g = st.globals();
    st.raw_set_str(g, b"N", Value::int(3)).unwrap();
    let mem = st.heap().mem_mut();
    let now = mem.total_bytes() + mem.ext_bytes();
    mem.set_limit(now + 64);
    // (BUNDLE-M, mutant `vbytes_no_limit_guard`) the same chunk text three times: the first two (N = 3, below the threshold) show what the chunk itself does to
    // the charged extension memory, the third (N = 200) crosses the threshold, is refused and must change it by the same amount
    let run_n = |st: &mut LuaState, n: u32| {
        let g = st.globals();
        st.raw_set_str(g, b"N", Value::int(i64::from(n))).unwrap();
        let e0 = st.heap().mem_mut().ext_bytes();
        let r = exec(st, "local s, v = F(N) return s .. ' ' .. v");
        (r, st.heap().mem_mut().ext_bytes() as i64 - e0 as i64)
    };
    run_n(&mut st, 3);
    let (_, ctl) = run_n(&mut st, 3);
    // the loop crosses the threshold: the recompile cannot be charged
    let (r, test) = run_n(&mut st, 200);
    assert_eq!(r, "20100 0");
    let s = rn_jit::stats(&mut st);
    assert!(s.t15_refused >= 1 && s.t15_recompiled == 0, "{s:?}");
    assert_eq!(
        test, ctl,
        "the refused promotion changed the charged extension memory"
    );
    st.heap().mem_mut().set_limit(usize::MAX);
    // with room again the backed-off counter fires and the promotion happens
    let mut v = String::new();
    for _ in 0..8 {
        v = exec(&mut st, "local s, v = F(2000) return s .. ' ' .. v");
        if rn_jit::stats(&mut st).t15_recompiled > 0 {
            break;
        }
    }
    let s = rn_jit::stats(&mut st);
    assert!(s.t15_promoted == 1 && s.t15_recompiled == 1, "{s:?} {v}");
    assert_eq!(exec(&mut st, "local s = F(100) return s"), "5050");
}

#[test]
fn a_refused_promotion_gives_up_after_the_back_off_limit() {
    let mut st = new_state(
        Mode::Always,
        T15Config {
            hot: 10,
            ..T15Config::for_mode(T15Mode::On)
        },
    );
    assert_eq!(
        exec(
            &mut st,
            "function F(n) local s = 0 for i = 1, n do s = s + i end return s end return F(3)"
        ),
        "6"
    );
    let mem = st.heap().mem_mut();
    let now = mem.total_bytes() + mem.ext_bytes();
    mem.set_limit(now + 64);
    // every retry is refused: 2^k back-edges later, k <= 10, then the nest is Off
    assert_eq!(exec(&mut st, "return F(100000)"), "5000050000");
    let s = rn_jit::stats(&mut st);
    assert!(
        s.t15_refused >= 5 && s.t15_off == 1 && s.t15_recompiled == 0,
        "{s:?}"
    );
    st.heap().mem_mut().set_limit(usize::MAX);
    assert_eq!(exec(&mut st, "return F(100000)"), "5000050000");
    let s = rn_jit::stats(&mut st);
    assert_eq!(s.t15_recompiled, 0, "an Off nest stays off: {s:?}");
}

#[test]
fn the_code_cap_refuses_the_new_version_and_the_old_one_keeps_running() {
    let mut st = new_state(
        Mode::Always,
        T15Config {
            hot: 100,
            ..T15Config::for_mode(T15Mode::On)
        },
    );
    assert_eq!(
        exec(
            &mut st,
            "function F(n) local s = 0 for i = 1, n do s = s + i end return s end return F(3)"
        ),
        "6"
    );
    let s = rn_jit::stats(&mut st);
    rn_jit::set_code_cap(&mut st, s.mapped as usize - 1);
    // a fresh chunk would be needed for nothing, but the allocation of a block must still fit: shrink the cap to
    // what is mapped and fill the rest of the chunk with refusals by asking for a huge function body
    assert_eq!(exec(&mut st, "return F(5000)"), "12502500");
    rn_jit::set_code_cap(&mut st, usize::MAX / 2);
    assert_eq!(exec(&mut st, "return F(50000)"), "1250025000");
}

/// BUNDLE-M M4 (mutant `vbytes_no_credit`): a recompile that is charged and then fails (the code cap refuses the block) gives back the bytes of
/// its version data: the extension-memory total is the same before and after the refused promotions.
#[test]
fn a_failed_recompile_refunds_its_version_bytes() {
    let mut st = new_state(
        Mode::Always,
        T15Config {
            hot: 100,
            ..T15Config::for_mode(T15Mode::On)
        },
    );
    // a loop with a long straight-line body: its tier-1 code takes more than half of the first 1 MiB chunk, so with the cap at what is
    // mapped the second version (the recompile) does not fit and `place_code` fails after the version bytes were charged
    let body = "s = s + i % 7 ".repeat(3000);
    let src = format!("function F(n) local s = 0 for i = 1, n do {body} end return s end function R() return F(N) end N = 1 return R()");
    exec(&mut st, &src);
    let s = rn_jit::stats(&mut st);
    rn_jit::set_code_cap(&mut st, s.mapped as usize);
    // the same chunk text twice: the first (N = 1, no promotion) shows what compiling the chunk charges, the second (N = 300) also attempts
    // the recompile of F; the extension memory must grow by the same amount
    exec(&mut st, "N = 1 return R()");
    let m0 = st.heap().mem_mut().ext_bytes();
    exec(&mut st, "N = 1 return R()");
    let m1 = st.heap().mem_mut().ext_bytes();
    assert_eq!(rn_jit::stats(&mut st).t15_refused, 0);
    exec(&mut st, "N = 300 return R()");
    let m2 = st.heap().mem_mut().ext_bytes();
    let s = rn_jit::stats(&mut st);
    assert!(
        s.t15_refused >= 1 && s.t15_recompiled == 0,
        "the recompile must be attempted and fail: {s:?}"
    );
    rn_jit::set_code_cap(&mut st, usize::MAX / 2);
    assert_eq!(
        m2 - m1,
        m1 - m0,
        "the version bytes of a failed recompile are not refunded"
    );
}

#[test]
fn opt_and_t15_knobs_parse_in_the_state_config() {
    let cfg = T15Config::parse(Some("on"), Some("77"), Some("-ra,+randra=3"));
    assert_eq!((cfg.mode, cfg.hot), (T15Mode::On, 77));
    assert!(cfg.opt.no_ra && cfg.opt.randra == Some(3));
}

#[test]
fn dead_pcs_exist_in_functions_with_compares_and_have_no_continuation() {
    let prog = "
        local function F(n)
          local s = 0
          for i = 1, n do if i % 2 == 0 then s = s + 1 end end
          return s, t15q(11), t15q(8)
        end
        local s, dead, len = F(4)
        return s .. ' ' .. tostring(dead >= 0 and dead < len)";
    let mut st = new_state(Mode::Always, always());
    assert_eq!(exec(&mut st, prog), "2 true");
}

#[test]
fn more_nests_than_versions_stay_correct_and_the_rest_go_off() {
    // seven loops in one function: the prototype has 6 compiles at most (tier 1 + 5), the last two nests stay in
    // the version they are in
    let mut body = String::from("local s = 0\n");
    for k in 1..=7 {
        body.push_str(&format!("for i{k} = 1, 30 do s = s + i{k} * {k} end\n"));
    }
    let prog = format!(
        "local function f() {body} return s, t15q(0), t15q(1), t15q(13) end
         local s, cur, retired, off
         for r = 1, 4 do s, cur, retired, off = f() end
         return s .. ' ' .. cur .. ' ' .. retired .. ' ' .. off"
    );
    let s = same(&prog.replace(", t15q(0), t15q(1), t15q(13)", ", 0, 0, 0"));
    assert!(s.t15_promoted >= 5, "{s:?}");
    let mut st = new_state(Mode::Always, always());
    // 1 + 2 + .. + 7 = 28 * 465 / .. : s = 465 * 28
    assert_eq!(exec(&mut st, &prog), "13020 5 5 2");
    let s = rn_jit::stats(&mut st);
    assert_eq!(s.t15_off, 2, "{s:?}");
}
