//! J2b end to end: generic `for` (pairs/ipairs/next/Lua iterators), vararg functions and VARARG, CLOSE/TBC and
//! RETURN with open upvalues inside real Lua programs. Every program runs in a jit-off twin state and in
//! `always` mode (plus tiny instruction-count budgets that force exits at arbitrary block boundaries) and must
//! produce the identical string. The whole binary runs with `RUSTYNAIL_JIT_STRESS=stackmove` behaviour (every
//! helper moves the stack), so a stale frame base after VARARGPREP/VARARG/CLOSE/TFORCALL fails here. Run the
//! binary also under `RUSTYNAIL_GC_STRESS=1` and `=2`.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode, Stats};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        assert!(
            rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
            "the compiler must be installed by this test binary"
        );
        rn_core::jit_hooks::set_stress_stackmove(true);
    });
}

fn run_with(src: &str, mode: Mode, budget: Option<i32>) -> (String, Stats) {
    init();
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, mode);
    if let Some(b) = budget {
        rn_jit::set_count_budget(&mut st, b);
    }
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
    (out, rn_jit::stats(&mut st))
}

fn same(src: &str) -> Stats {
    let (off, s_off) = run_with(src, Mode::Off, None);
    assert_eq!(s_off.enters, 0);
    assert!(!off.starts_with("ERROR"), "program failed: {off}");
    let (on, s_on) = run_with(src, Mode::Always, None);
    assert_eq!(off, on, "jit-on result differs from jit-off for:\n{src}");
    for b in [3, 17, 200] {
        let (on, _) = run_with(src, Mode::Always, Some(b));
        assert_eq!(off, on, "budget {b}: jit-on result differs for:\n{src}");
    }
    assert!(s_on.compiled >= 1 && s_on.enters >= 1, "{s_on:?}");
    s_on
}

#[test]
fn pairs_next_with_modification() {
    same(
        r#"
        local t = {}
        for i = 1, 50 do t[i] = i end
        for i = 1, 20 do t['k' .. i] = i end
        local cnt, sum = 0, 0
        for k, v in pairs(t) do cnt = cnt + 1; sum = sum + v; t[k] = v * 2 end   -- assign existing fields
        local sum2 = 0
        for k, v in next, t do sum2 = sum2 + v end
        local c2 = 0
        for k, v in pairs(t) do t[k] = nil; c2 = c2 + 1 end                       -- clear while traversing
        local left = 0
        for _ in pairs(t) do left = left + 1 end
        local u = {10, 20, 30, nil, 50, x = 1, y = 2}
        local order = {}
        for k, v in pairs(u) do if type(k) == 'number' then order[#order + 1] = k .. '=' .. v end end
        local only_keys = 0
        for k in pairs({a = 1, b = 2, c = 3}) do only_keys = only_keys + 1 end
        local total = 0
        for r = 1, 300 do for k, v in pairs({r, r + 1, r + 2}) do total = total + k * v end end
        return cnt .. ',' .. sum .. ',' .. sum2 .. ',' .. c2 .. ',' .. left .. ',' .. tostring(next(t))
          .. ',' .. table.concat(order, ';') .. ',' .. only_keys .. ',' .. total
    "#,
    );
}

#[test]
fn pairs_metamethod_and_other_iterators() {
    same(
        r#"
        local p = setmetatable({}, {__pairs = function(t) return function(_, k) if not k then return 1, 'one' elseif k < 3 then return k + 1, 'more' end end, t, nil end})
        local r = {}
        for k, v in pairs(p) do r[#r + 1] = k .. v end
        local function range(n) return function(_, i) if i < n then return i + 1 end end, nil, 0 end
        local s = 0
        for i in range(500) do s = s + i end
        local function stateful(t) local i = 0 return function() i = i + 1 return t[i] end end
        local z = 0
        for v in stateful({4, 5, 6}) do z = z + v end
        local tt = setmetatable({}, {__index = function(_, i) if i <= 3 then return i * 11 end end})
        local y = 0
        for i, v in ipairs(tt) do y = y + v end
        local str = 0
        for w in string.gmatch('a bb ccc', '%a+') do str = str + #w end
        local multi = 0
        for a, b, c, d in function(_, i) if i < 3 then return i + 1, 2, 3, 4 end end, nil, 0 do multi = multi + a + b + c + d end
        return table.concat(r, ',') .. '|' .. s .. '|' .. z .. '|' .. y .. '|' .. str .. '|' .. multi
    "#,
    );
}

#[test]
fn ipairs_holes_and_growth() {
    same(
        r#"
        local t = {1, 2, 3, nil, 5}
        local n = 0
        for i, v in ipairs(t) do n = n + 1 end
        local p = setmetatable({}, {__index = function(_, i) if i <= 5 then return i * 10 end end})
        local m = 0
        for i, v in ipairs(p) do m = m + v end
        local a = {1, 2, 3}
        local k = 0
        for i, v in ipairs(a) do k = k + 1; if i < 10 then a[#a + 1] = i end end
        local big = {}
        for i = 1, 1000 do big[i] = i end
        local bs = 0
        for r = 1, 20 do for i, v in ipairs(big) do bs = bs + v end end
        local s = 0
        for i, v in ipairs({}) do s = s + 1 end
        local h = {}
        h[1] = 'a' h[2] = 'b' h[4] = 'd'
        local hs = ''
        for i, v in ipairs(h) do hs = hs .. v end
        local fl = 0
        for i, v in ipairs({10, 20, 30}) do fl = fl + i * v end
        return n .. ',' .. m .. ',' .. k .. ',' .. #a .. ',' .. bs .. ',' .. s .. ',' .. hs .. ',' .. fl
    "#,
    );
}

#[test]
fn ipairs_inline_cases() {
    // the inline array step: one/two/three loop variables, all value types, tables that change under the
    // loop (array growth and shrink, clearing, hash-part elements), metatables, non-table states
    same(
        r#"
        local r = {}
        local t = {10, 'two', 3.5, true, false, {}, print, 8}
        local n = 0
        for i in ipairs(t) do n = n + i end
        local s = ''
        for i, v in ipairs(t) do s = s .. i .. type(v) .. ',' end
        local x = 0
        for i, v, w in ipairs(t) do x = x + (w == nil and 1 or 100) end
        r[#r + 1] = n .. '|' .. s .. '|' .. x
        local g = {1, 2, 3}
        local seen = 0
        for i, v in ipairs(g) do seen = seen + 1; if i == 2 then g[2] = nil end end
        local h = {1, 2, 3, 4, 5, 6}
        local seen2 = 0
        for i, v in ipairs(h) do seen2 = seen2 + 1; h[#h] = nil end
        local k = {1, 2}
        local seen3 = 0
        for i, v in ipairs(k) do seen3 = seen3 + 1; if seen3 < 20 then k[#k + 1] = seen3 end end
        local m = {}
        for i = 1, 5 do m[i] = i end
        m[7] = 7
        local seen4 = 0
        for i, v in ipairs(m) do seen4 = seen4 + v end
        local u = {}
        u[3] = 'c' u[2] = 'b' u[1] = 'a'
        local us = ''
        for i, v in ipairs(u) do us = us .. v end
        local mt = setmetatable({1, 2}, {__index = function(_, i) return i < 5 and i * 100 or nil end})
        local ms = 0
        for i, v in ipairs(mt) do ms = ms + v end
        local prox = setmetatable({}, {__index = {5, 6, 7}})
        local ps = 0
        for i, v in ipairs(prox) do ps = ps + v end
        local cnt = 0
        for _ = 1, 400 do for i, v in ipairs(t) do cnt = cnt + i end end
        r[#r + 1] = seen .. ',' .. seen2 .. ',' .. seen3 .. ',' .. seen4 .. ',' .. us .. ',' .. ms .. ',' .. ps .. ',' .. cnt
        local big = {}
        for i = 1, 5000 do big[i] = i end
        local bs = 0
        for i, v in ipairs(big) do bs = bs + v end
        local e = 0
        for i, v in ipairs({}) do e = e + 1 end
        r[#r + 1] = bs .. ',' .. e
        local ok, err = pcall(function() for i, v in ipairs(nil) do end end)
        r[#r + 1] = tostring(ok) .. tostring(err)
        return table.concat(r, ';')
    "#,
    );
}

#[test]
fn next_errors_and_non_iterators() {
    same(
        r#"
        local r = {}
        local function try(f) local ok, e = pcall(f) r[#r + 1] = tostring(ok) .. ':' .. tostring(e) end
        try(function() for k in next, {}, 'nokey' do end end)
        try(function() local t = {a = 1}; for k, v in next, t, 'zz' do end end)
        try(function() for k, v in next, nil do end end)
        try(function() for k, v in 5 do end end)
        try(function() for k, v in pairs(nil) do end end)
        try(function() for i, v in ipairs() do end end)
        try(function() local t = {1, 2, 3}; for k, v in next, t, 99 do end end)
        try(function() for k, v in next, {10}, 1.5 do end end)
        try(function() for k, v in nil do end end)
        try(function() local t = {1, 2, 3}; for k, v in next, t do t.new = 1 end end)
        try(function() for i, v in ipairs(nil) do end end)
        try(function() for i, v in ipairs(42) do end end)
        try(function() for i, c in ipairs('abc') do end end)
        return table.concat(r, '|')
    "#,
    );
}

#[test]
fn varargs() {
    same(
        r#"
        local function va(...) return select('#', ...), ... end
        local function pass(...) return ... end
        local function first(...) local a = ... return a end
        local function cnt(...) return select('#', ...) end
        local function pk(...) return table.pack(...) end
        local function mid(a, ...) local x, y = ... return a, x, y, select('#', ...) end
        local function tail(...) return pass(...) end
        local function tbl(...) return {...}, {n = select('#', ...), ...} end
        local r = {}
        for i = 0, 8 do
          local args = {}
          for j = 1, i do args[j] = j * 3 end
          local n, a, b = va(table.unpack(args, 1, i))
          r[#r + 1] = n .. ':' .. tostring(a) .. ':' .. tostring(b) .. ':' .. cnt(table.unpack(args, 1, i))
            .. ':' .. pk(tail(table.unpack(args, 1, i))).n .. ':' .. #tbl(table.unpack(args, 1, i))
            .. ':' .. select('#', mid(table.unpack(args, 1, i)))
        end
        r[#r + 1] = cnt(nil, nil, nil) .. cnt(pass(nil, nil)) .. pk(nil, nil, nil).n
        local big = {}
        for i = 1, 300 do big[i] = i end
        r[#r + 1] = cnt(table.unpack(big)) .. ':' .. select(300, table.unpack(big)) .. ':' .. select(-1, table.unpack(big))
        local x, y = ...
        r[#r + 1] = tostring(x) .. tostring(y) .. select('#', ...)
        local function sumv(n, ...) if n == 0 then return select('#', ...) end return sumv(n - 1, n, ...) end
        r[#r + 1] = sumv(100)
        local acc = 0
        for i = 1, 500 do acc = acc + cnt(i, i) + first(i, 2) + (select(2, i, i + 1)) end
        r[#r + 1] = acc
        r[#r + 1] = table.concat({pass(1, 2, 3)}, '') .. select('#', pass()) .. (pass(7, 8, 9))
        r[#r + 1] = math.max(pass(3, 9, 4)) .. string.format('%d-%d', pass(5, 6)) .. string.char(pass(72, 105))
        local ok, v1, v2 = pcall(pass, 'a', 'b')
        r[#r + 1] = tostring(ok) .. v1 .. v2
        local function many(...) local t = {...} return #t, ... end
        r[#r + 1] = select('#', many(table.unpack(big, 1, 200)))
        return table.concat(r, '|')
    "#,
    );
}

#[test]
fn vararg_callee_from_compiled_and_interpreted_code() {
    // the same vararg functions are called from a hot compiled loop (flat calls), from the interpreter
    // (`off` twin, and cold calls) and through pcall / coroutines
    same(
        r#"
        local function v(a, b, ...) return a, b, select('#', ...), ... end
        local function w(...) local n = select('#', ...) return n + (... or 0) end
        local t = {}
        for i = 1, 400 do
          local a, b, c = v(i)
          local d, e, f, g = v(i, i, i, i)
          t[#t + 1] = (a or 0) + (b or 0) + c + d + f + (g or 0) + w(i) + w()
        end
        local s = 0
        for _, x in ipairs(t) do s = s + x end
        local co = coroutine.wrap(function(...)
          local args = {...}
          for i = 1, #args do coroutine.yield(w(table.unpack(args, i))) end
          return 'end'
        end)
        local out = {}
        out[1] = co(5, 6, 7)
        out[2] = co()
        out[3] = co()
        out[4] = co()
        -- B == 0 calls (multret argument list) with exactly one argument too few: the flat call must nil-fill
        local function two(a, b) return tostring(a) .. tostring(b) end
        local function one() return 1 end
        local function three(x, y, z, ...) return tostring(z) .. select('#', ...) end
        local function pass(...) return ... end
        local o = {}
        for i = 1, 300 do o[#o + 1] = two(one()) .. three(pass(1, 2)) .. three(pass(1, 2, 3, 4, 5)) .. three(pass()) end
        local ok, err = pcall(v)
        local ok2, err2 = pcall(function(...) local a <const> = ... error('x' .. select('#', ...), 0) end, 1, 2, 3)
        return s .. '|' .. o[1] .. o[300] .. '|' .. table.concat(out, ',') .. '|' .. tostring(ok) .. '|' .. tostring(err2)
    "#,
    );
}

#[cfg(not(rustynail_lua54))]
#[test]
fn named_vararg_tables() {
    // Lua 5.5 `...name` parameters (PF_VATAB): the table is built by VARARGPREP's helper
    same(
        r#"
        local function f(a, ...t) return a, t.n, t[1], #t end
        local function g(...args) local s = 0 for i = 1, args.n do s = s + args[i] end return s end
        local r = {}
        for i = 1, 100 do r[#r + 1] = select('#', f(i, i, i + 1)) .. g(i, 2, 3) end
        return table.concat(r, ',', 1, 5) .. #r .. g() .. g(1, 2, 3, 4, 5, 6, 7, 8, 9, 10)
    "#,
    );
}

#[test]
fn closing_upvalues_in_loops() {
    same(
        r#"
        local fs = {}
        for i = 1, 10 do local x = i * 2; fs[#fs + 1] = function() x = x + 1; return i + x end end
        local s = 0
        for _, f in ipairs(fs) do s = s + f() + f() end
        local gs = {}
        local i = 1
        while i <= 10 do
          local y = i
          gs[#gs + 1] = function() return y end
          if i == 7 then goto done end
          i = i + 1
        end
        ::done::
        local t2 = 0
        for _, g in ipairs(gs) do t2 = t2 + g() end
        local hs = {}
        for k = 1, 100 do local z = k; hs[#hs + 1] = function() return z end; if k == 5 then break end end
        local u = 0
        for _, h in ipairs(hs) do u = u + h() end
        local n = 0
        repeat local w = n; local f = function() return w end; n = n + 1 until f() >= 5
        local cl = {}
        for k, v in pairs({a = 1, b = 2, c = 3}) do cl[#cl + 1] = function() return v end end
        local w2 = 0
        for _, c in ipairs(cl) do w2 = w2 + c() end
        local function mk(m) local c = 0; return function() c = c + m; return c end end
        local m1, m2 = mk(1), mk(10)
        local v = 0
        for _ = 1, 5 do v = v + m1() + m2() end
        local function ret_closure(k) local a, b = k, k * 2; return function() return a + b end, a end
        local rc = 0
        for k = 1, 200 do local f, a = ret_closure(k); rc = rc + f() + a end
        local function nested()
          local res = {}
          for a = 1, 3 do
            for b = 1, 3 do
              local c = a * b
              res[#res + 1] = function() return c end
              if b == 2 then break end
            end
          end
          local sum = 0
          for _, f in ipairs(res) do sum = sum + f() end
          return sum
        end
        return s .. ',' .. t2 .. ',' .. u .. ',' .. n .. ',' .. w2 .. ',' .. v .. ',' .. rc .. ',' .. nested()
    "#,
    );
}

#[test]
fn to_be_closed_variables() {
    same(
        r#"
        local log = {}
        local function closer(name)
          return setmetatable({}, {__close = function(_, e) log[#log + 1] = name .. ':' .. tostring(e) end})
        end
        local function f1() local a <close> = closer('a'); local b <close> = closer('b'); return 1, 2 end
        local function f2() local a <close> = closer('a2'); error('E', 0) end
        local function f3() for i = 1, 3 do local c <close> = closer('c' .. i); if i == 2 then break end end return 'x' end
        local function f4(...) local a <close> = closer('v'); return ... end
        local function f5() local x <close> = nil; local y <close> = false; return 5 end
        local function f6() local ok, e = pcall(function() local z <close> = 42 end) return e end
        local function it() return nil end
        local function f7() for k in it, nil, nil, closer('gen') do end return 7 end
        local function f8() local a <close> = closer('g'); return 1, 2, 3 end
        local function f9() for k, v in pairs({1}) do local q <close> = closer('q' .. k); goto out end ::out:: return 9 end
        for r = 1, 3 do
          f1(); pcall(f2); f3(); f4(1, 2, 3); f5(); f6(); f7(); f9()
        end
        local p, q, s = f8()
        local function h() local a <close> = setmetatable({}, {__close = function() error('inclose', 0) end}); return 1 end
        local ok, e = pcall(h)
        local function deep(n) local a <close> = closer('d' .. n); if n > 0 then return deep(n - 1) end return n end
        local dd = deep(3)
        local multi = select('#', f4(1, nil, nil))
        return table.concat(log, ',') .. '|' .. p .. q .. s .. '|' .. tostring(ok) .. tostring(e) .. '|' .. f6() .. '|' .. dd .. multi
    "#,
    );
}

#[test]
fn coroutines_and_generic_for() {
    same(
        r#"
        local function gen(n) return coroutine.wrap(function() for i = 1, n do coroutine.yield(i, i * i) end end) end
        local s = 0
        for a, b in gen(20) do s = s + a + b end
        local co = coroutine.create(function(...)
          local t = {10, 20, 30, x = 1}
          local r = select('#', ...)
          for k, v in pairs(t) do r = r + (coroutine.yield(k) or 0) end
          for i, v in ipairs(t) do r = r + coroutine.yield(i, v) end
          return r, ...
        end)
        local out = {}
        local ok, v = coroutine.resume(co, 'a', 'b')
        local n = 0
        while coroutine.status(co) ~= 'dead' and n < 100 do
          out[#out + 1] = tostring(ok) .. tostring(v); n = n + 1; ok, v = coroutine.resume(co, n)
        end
        local co2 = coroutine.wrap(function()
          local c <close> = setmetatable({}, {__close = function() coroutine.yield('closing') end})
          return 'ret'
        end)
        local y1 = co2(); local y2 = co2()
        local co3 = coroutine.wrap(function()
          for v in function(_, c) c = (c or 0) + 1; if c > 3 then return nil end coroutine.yield('it' .. c); return c end do end
          return 'done'
        end)
        local z = {}
        for i = 1, 4 do z[#z + 1] = co3() end
        local co4 = coroutine.wrap(function(...)
          local function f(...) return coroutine.yield(...) end
          local a, b = f(...)
          return a, b, select('#', ...)
        end)
        local q1, q2 = co4(1, 2, 3)
        local r1, r2, r3 = co4('x', 'y')
        return s .. '|' .. table.concat(out, ',') .. '|' .. y1 .. y2 .. '|' .. table.concat(z, ',')
          .. '|' .. tostring(q1) .. tostring(q2) .. '|' .. tostring(r1) .. tostring(r2) .. tostring(r3)
    "#,
    );
}

#[test]
fn natives_with_multret_and_variable_counts() {
    same(
        r#"
        local function f(...) return select('#', ...), pcall(select, '#', ...) end
        local r = {}
        for i = 1, 100 do local n, ok, m = f(i, nil, i) r[#r + 1] = n .. tostring(ok) .. m end
        local function g(...) return math.max(...), math.min(...), type(...), tostring(...) end
        for i = 1, 100 do r[#r + 1] = table.concat({g(i, i + 1, i - 1)}, ',') end
        local function h(...) local t = table.pack(...) return t.n, ... end
        r[#r + 1] = select('#', h()) .. select('#', h(nil)) .. select('#', h(1, 2, 3))
        r[#r + 1] = select('#', select(2, 'a', 'b', 'c')) .. select(-2, 'a', 'b', 'c')
        r[#r + 1] = tostring(select('#', table.unpack({}, 1, 100))) .. '/' .. select('#', ('x'):byte(1, -1))
        local ok, e = pcall(select, 0)
        r[#r + 1] = tostring(ok) .. tostring(e)
        local ok2, e2 = pcall(function(...) return select(-5, ...) end, 1, 2)
        r[#r + 1] = tostring(ok2) .. tostring(e2)
        r[#r + 1] = table.concat({string.byte('hello', 1, -1)}, ',') .. string.rep('ab', 3, '-') .. #{table.unpack({1, 2, 3}, 2)}
        return table.concat(r, '|')
    "#,
    );
}

#[test]
fn error_positions_through_vararg_frames() {
    same(
        r#"
        local r = {}
        local function try(f, ...) local ok, e = pcall(f, ...) r[#r + 1] = tostring(e) end
        try(function(...) local t = nil; return t.x, ... end, 1, 2)
        try(function(...) return (...) + {} end, 1)
        try(function(...) local a, b = ...; return a < b end, 1, 'x')
        try(function(...) error('lvl2', 2) end)
        try(function(...) local x <close> = 5 end)
        try(function(...) for i = 1, 10 do local a = select(i, ...) end end, 1, 2)
        try(function(...) for k, v in pairs(...) do end end, 7)
        local ok, tb = xpcall(function(...) local t = nil; t.x = 1 end, debug.traceback, 1, 2, 3)
        r[#r + 1] = tb
        local function lvl(...) return debug.getinfo(1, 'nSl').currentline, debug.getinfo(1, 'u').isvararg, debug.getinfo(1, 'u').nparams end
        local a1, a2, a3 = lvl(1, 2, 3)
        r[#r + 1] = a1 .. tostring(a2) .. a3
        return table.concat(r, '|')
    "#,
    );
}

/// Quantitative check that the new templates keep loops compiled: with every instruction of these hot loops
/// being a template, the compiled code must not bounce through the interpreter per iteration.
#[test]
fn hot_loops_stay_compiled() {
    let src = r#"
        local t = {}
        for i = 1, 100 do t[i] = i end
        t.a = 1 t.b = 2
        local function va(...) local x = ... return x end
        local acc = 0
        for r = 1, 300 do
          for i, v in ipairs(t) do acc = acc + v end
          for k, v in pairs(t) do acc = acc + 1 end
          acc = acc + va(r, 2)
        end
        return acc
    "#;
    let (out, s) = run_with(src, Mode::Always, None);
    assert_eq!(out, run_with(src, Mode::Off, None).0);
    // 300 * (100 + 102) = 60600 loop iterations; a per-iteration exit would show up as >= tens of thousands
    assert!(
        s.exit_continue < 1000,
        "too many exits to the interpreter: {s:?}"
    );
}
