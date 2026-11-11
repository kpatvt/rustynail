//! J1d end to end: the table templates inside real Lua programs, compared against the jit-off twin state (the
//! whole binary runs in `always` mode). GC torture: run this binary with `RUSTYNAIL_GC_STRESS=1` and `=2`
//! (the heap reads the variable when a state is created); the programs switch between the incremental and
//! the generational collector themselves and write fresh objects into old tables while collecting.
#![cfg(feature = "jit")]

use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_jit::{Config, Mode, Stats};
use std::sync::Once;

fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        #[cfg(feature = "tr_mutants")]
        rn_jit::tr::mutants::apply_vm_mutants();
        assert!(
            rn_jit::register_with_trace_pinned_off(Config::for_mode(Mode::Always)),
            "the compiler must be installed by this test binary"
        );
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
        format!("ERROR {status:?}: {v:?}")
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
    s_on
}

#[test]
fn array_loops() {
    let s = same(
        r#"
        local t = {}
        for i = 1, 300 do t[i] = i * 2 end
        local sum = 0
        for r = 1, 5 do for i = 1, #t do sum = sum + t[i] end end
        for i = 1, #t do t[i] = t[i] + 1.5 end
        local f = 0
        for i = 1, #t do f = f + t[i] end
        local u = {}
        for i = 1, 100 do u[#u + 1] = i end
        local n = {}
        for i = -3, 5 do n[i] = i end
        local h = 0
        for i = -3, 5 do h = h + n[i] end
        local fk = {10, 20, 30}
        return sum .. "," .. f .. "," .. #u .. "," .. h .. "," .. fk[2.0] .. "," .. tostring(fk[0]) .. tostring(fk[4])
    "#,
    );
    assert!(s.compiled >= 1 && s.enters >= 1, "{s:?}");
    eprintln!("{s:?}");
}

#[test]
fn matrices_and_fields() {
    same(
        r#"
        local n = 20
        local function mk(v) local m = {} for i = 1, n do local r = {} for j = 1, n do r[j] = v + i - j end m[i] = r end return m end
        local a, b = mk(1), mk(2)
        local c = {}
        for i = 1, n do local ci = {} c[i] = ci
          for j = 1, n do local s = 0 for k = 1, n do s = s + a[i][k] * b[k][j] end ci[j] = s end end
        local tr = 0
        for i = 1, n do tr = tr + c[i][i] end
        local p = {x = 1.5, y = 2, name = "p", z = 0}
        for i = 1, 200 do p.x = p.x + p.y; p.z = p.z + i; p.y = p.y * 1 end
        return tr .. "," .. p.x .. "," .. p.z .. "," .. p.name
    "#,
    );
}

#[test]
fn methods_and_metatables() {
    same(
        r#"
        local C = {}
        C.__index = C
        function C.new(v) return setmetatable({v = v, log = {}}, C) end
        function C:add(d) self.v = self.v + d return self end
        function C:get() return self.v end
        local o = C.new(1)
        for i = 1, 100 do o:add(i) end
        local calls = 0
        local P = setmetatable({}, {__index = function(t, k) calls = calls + 1 return k * 2 end,
                                    __newindex = function(t, k, v) calls = calls + 100 rawset(t, k, v) end})
        local s = 0
        for i = 1, 50 do s = s + P[i] end
        for i = 1, 50 do P[i] = i end
        for i = 1, 50 do s = s + P[i] end
        local D = setmetatable({}, {__index = C})
        D.v = 5
        return o:get() .. "," .. s .. "," .. calls .. "," .. D:get() .. "," .. tostring(rawget(P, 3))
    "#,
    );
}

#[test]
fn globals_gettabup_settabup() {
    same(
        r#"
        counter = 0
        name = "n"
        for i = 1, 500 do counter = counter + i; other = (other or 0) + 1 end
        local s = 0
        for i = 1, 100 do s = s + math.floor(i / 3) + #name end
        return counter .. "," .. other .. "," .. s
    "#,
    );
}

#[test]
fn errors_and_edge_keys() {
    same(
        r#"
        local out = {}
        local function try(f, ...) local ok, e = pcall(f, ...) out[#out + 1] = tostring(ok) .. ":" .. tostring(e) end
        local t = {1, 2, 3, x = 1}
        local n
        for i = 1, 5 do
          try(function() return n.x end)
          try(function() return n[i] end)
          try(function() n.x = i end)
          try(function() return t.x.y.z end)
          try(function() return t[nil] end)
          try(function() t[nil] = 1 end)
          try(function() t[0/0] = 1 end)
          try(function() return (1)[i] end)
          try(function() return ("abc"):upper() end)
          try(function() return t:nomethod() end)
          try(function() return (nil):foo() end)
        end
        local big = {}
        big[1 << 40] = 1; big[math.maxinteger] = 2; big[math.mininteger] = 3
        out[#out + 1] = big[1 << 40] + big[math.maxinteger] + big[math.mininteger]
        return table.concat(out, "|")
    "#,
    );
}

#[test]
fn holes_and_borders() {
    same(
        r#"
        local t = {1, 2, nil, 4, nil, nil, 7}
        local r = {}
        for i = 1, 9 do r[#r + 1] = tostring(t[i]) end
        t[3] = 3; t[5] = 5
        for i = 1, 9 do r[#r + 1] = tostring(t[i]) end
        for i = 1, 7 do t[i] = nil end
        r[#r + 1] = #t
        local u = table.pack(1, 2, 3)
        u[2] = nil
        r[#r + 1] = tostring(u[2]) .. u.n
        return table.concat(r, ",")
    "#,
    );
}

/// Fresh collectable objects written into old (black) tables while the collector runs, both collectors.
#[test]
fn gc_barriers_in_loops() {
    for mode in ["incremental", "generational"] {
        let src = format!(
            r#"
            collectgarbage("{mode}")
            local old = {{}}
            local reg = {{}}
            local slots = {{}}
            for i = 1, 64 do old[i] = false; slots[i] = false end
            old.field = false; old.f2 = false
            collectgarbage(); collectgarbage()   -- old tables are black / old now
            for round = 1, 40 do
              for i = 1, 64 do
                old[i] = {{ round, i, tostring(i) }}        -- SETTABLE / SETI with a fresh table
                slots[i] = "s" .. i .. "r" .. round     -- fresh string
              end
              old.field = {{ round }}
              old.f2 = function() return round end
              if round % 3 == 0 then collectgarbage("step", 1) end
              if round % 7 == 0 then collectgarbage() end
              reg[#reg % 5 + 1] = old[round % 64 + 1]
              local junk = {{}}
              for i = 1, 200 do junk[i] = {{ }} end
            end
            local sum = 0
            for i = 1, 64 do sum = sum + old[i][1] + old[i][2] + #old[i][3] + #slots[i] end
            return sum .. "," .. old.field[1] .. "," .. old.f2() .. "," .. #reg
        "#
        );
        same(&src);
    }
}

/// The interpreter and compiled code interleave on the same table: a proto is compiled, the test body mutates the
/// shape (array growth, hash resize, key removal) between runs of the compiled function.
#[test]
fn shapes_change_between_runs() {
    same(
        r#"
        local function get(t, k) return t[k] end
        local function geti(t) return t[3] end
        local function getf(t) return t.name end
        local function set(t, k, v) t[k] = v end
        local function setf(t, v) t.name = v end
        local t = {1, 2, 3}
        local out = {}
        for round = 1, 30 do
          out[#out + 1] = tostring(geti(t)) .. tostring(getf(t)) .. tostring(get(t, round % 5))
          if round == 5 then t.name = "n" end
          if round == 8 then t[3] = nil end
          if round == 10 then for i = 4, 40 do t[i] = i end end
          if round == 15 then setmetatable(t, {__index = function(_, k) return "mm" .. tostring(k) end}) end
          if round == 20 then t.name = nil; t[3] = nil end
          if round == 25 then for i = 1, 40 do t["k" .. i] = i end t.name = "again" end
          set(t, round, round); setf(t, round * 2)
          if round % 4 == 0 then collectgarbage() end
        end
        return table.concat(out, ",")
    "#,
    );
}

/// Lua 5.4 luaH_getint moves the array limit hint on array accesses, which changes later '#t' on arrays with
/// holes; the inline accesses must do the same update (or exit): random access sequences, '#t' observed
/// after every step, through GETTABLE / SETTABLE / GETI / SETI / the ipairs iterator.
#[test]
fn array_limit_hint_sequences() {
    same(
        r#"
        local seed = 12345
        local function rnd(n) seed = (seed * 1103515245 + 12345) % 2147483648; return seed % n + 1 end
        local out = {}
        for round = 1, 80 do
          local n = rnd(40)
          local t = {}
          for i = 1, n do t[i] = i end
          for k = 1, rnd(6) do t[rnd(n)] = nil end
          local _ = #t
          for step = 1, 40 do
            local k = rnd(n + 3)
            local op = rnd(8)
            if op == 1 then local v = t[k]
            elseif op == 2 then t[k] = step
            elseif op == 3 then t[k] = nil
            elseif op == 4 then local v = t[6]; t[7] = step; local w = t[8]
            elseif op == 5 then for i, v in ipairs(t) do if i > 9 then break end end
            elseif op == 6 then t[5] = nil; t[9] = t[10]
            else out[#out + 1] = #t end
          end
          out[#out + 1] = #t
        end
        return table.concat(out, ",")
    "#,
    );
}

/// T2: instance-miss / `__index` chain lookups of the tier-1 slow helper (`jit_get`, short-string keys):
/// chain depth around the inline limit, function `__index` in the middle, absent `__index`, metatable
/// changes between lookups, `__index` cycle ("loop in gettable"), `__newindex` on absent keys, rawset
/// interplay, SELF on instances and strings, and a global lookup (GETTABUP) through `_ENV`'s `__index`.
#[test]
fn t2_index_miss_chains() {
    same(
        r#"
        local function chain(n)
          local top, t = {}, nil
          t = top
          for i = 1, n do
            local nxt = {["k" .. i] = i}
            setmetatable(t, {__index = nxt})
            t = nxt
          end
          return top
        end
        local out = {}
        for _, depth in ipairs{1, 3, 4, 5, 6, 9} do
          local top = chain(depth)
          local s = 0
          for r = 1, 30 do
            for i = 1, depth do
              local v = top["k" .. i]
              s = s + (v or 1000)
            end
            if top.zzz == nil then s = s + 1 end
          end
          out[#out + 1] = s
        end
        -- literal-key chains: GETFIELD
        local A = {a = 1}
        local B = setmetatable({b = 2}, {__index = A})
        local C = setmetatable({c = 3}, {__index = B})
        local inst = setmetatable({}, {__index = C})
        local s2 = 0
        for i = 1, 100 do s2 = s2 + inst.a + inst.b + inst.c + (inst.d or 0) end
        out[#out + 1] = s2
        -- the chain changes between lookups, absent __index, function in the middle
        local m = {x = 1}
        local o = setmetatable({}, {__index = setmetatable({}, {__index = m})})
        local acc = 0
        for i = 1, 60 do
          if i == 20 then m.x = 2 end
          if i == 30 then getmetatable(o).__index = function(_, k) return 7 end end
          if i == 40 then getmetatable(o).__index = nil end
          if i == 50 then getmetatable(o).__index = m end
          acc = acc + (o.x or -1)
        end
        out[#out + 1] = acc
        -- cycle
        local c1, c2 = {}, {}
        setmetatable(c1, {__index = c2}); setmetatable(c2, {__index = c1})
        local ok, err
        for i = 1, 5 do ok, err = pcall(function() return c1.nothing end) end
        out[#out + 1] = tostring(ok) .. ":" .. tostring(err):gsub("^.-:%d+: ", "")
        -- __newindex on absent keys + rawset
        local log = 0
        local N = setmetatable({}, {__newindex = function(t, k, v) log = log + 1; rawset(t, k, v * 2) end})
        for i = 1, 50 do N.f = i; N.g = i; N.f = i end
        out[#out + 1] = log .. "," .. N.f .. "," .. N.g
        local R = setmetatable({}, {__index = {z = 5}, __newindex = function() error("no") end})
        rawset(R, "z", 9)
        local zs = 0
        for i = 1, 40 do zs = zs + R.z end
        out[#out + 1] = zs
        -- SELF on instances and strings
        local K = {}
        K.__index = K
        function K:get() return self.v end
        local k = setmetatable({v = 3}, K)
        local sm = 0
        for i = 1, 100 do sm = sm + k:get() + #("x"):rep(2) end
        out[#out + 1] = sm
        -- GETTABUP through _ENV with __index
        local env = setmetatable({}, {__index = function(_, n) return #n end})
        local f = load("local s = 0 for i = 1, 50 do s = s + abc + defgh end return s", "x", "t", env)
        out[#out + 1] = f()
        -- value of a non-table in the chain
        local nt = setmetatable({}, {__index = "str"})
        out[#out + 1] = tostring(nt.len ~= nil)
        return table.concat(out, ";")
    "#,
    );
}

/// C2: `LT` / `LE` / `EQ` of non-numbers stay in tier 1 (`jit_cmp`): strings (short, long, empty, embedded NULs, prefixes), mixed
/// operand kinds in one loop (the template sees every type pair), tables with and without `__eq` / `__lt` / `__le` (the helper
/// answers only the metamethod-free cases), userdata, functions, booleans, nil, NaN, and the error messages of invalid comparisons.
#[test]
fn c2_tier1_compares() {
    same(
        r#"
        local long1 = ("abc"):rep(30) .. "x"
        local long2 = ("abc"):rep(30) .. "y"
        local long3 = ("abc"):rep(30) .. "x"
        local vals = {"", "a", "ab", "b", "a\0b", "a\0", long1, long2, long3, "abc", 1, 2.5, 3, 0/0, true, false, io.stdout,
                      print, {}, {}, setmetatable({}, {__eq = function() return true end}), setmetatable({}, {__eq = function() return true end})}
        local out = {}
        for r = 1, 3 do
          local acc = 0
          for i = 1, #vals do
            for j = 1, #vals do
              local a, b = vals[i], vals[j]
              if a == b then acc = acc + 1 end
              if a ~= b then acc = acc + 2 end
              local ok, res = pcall(function() return a < b end)
              if ok and res then acc = acc + 4 elseif not ok then acc = acc + 8 end
              ok, res = pcall(function() return a <= b end)
              if ok and res then acc = acc + 16 elseif not ok then acc = acc + 32 end
            end
          end
          out[#out + 1] = acc
        end
        -- hot loops over homogeneous operands (the tier-1 template runs the helper)
        local S = {}
        for k = 1, 64 do S[k] = "key" .. (k * 7 % 64) end
        local lt, le, eq, ne = 0, 0, 0, 0
        for r = 1, 40 do
          for k = 1, 63 do
            local a, b = S[k], S[k + 1]
            if a < b then lt = lt + 1 end
            if a <= b then le = le + 1 end
            if a == b then eq = eq + 1 end
            if S[k] ~= S[(k + r) % 64 + 1] then ne = ne + 1 end
          end
        end
        out[#out + 1] = lt .. "," .. le .. "," .. eq .. "," .. ne
        -- the loop changes the operand kinds half way
        local mix = {}
        for i = 1, 200 do
          local a, b = i, i + 1
          if i > 100 then a, b = "s" .. i, "s" .. (i + 1) end
          if i > 150 then a, b = {}, a end
          local ok, r2 = pcall(function() return a < b end)
          mix[#mix + 1] = ok and tostring(r2) or r2:gsub("^.-:%d+: ", "")
        end
        out[#out + 1] = table.concat(mix, "|"):len() .. ":" .. mix[1] .. mix[120] .. mix[180]
        -- metamethods: __lt / __le / __eq switched on and off under a hot loop
        local mt = {}
        local x, y = setmetatable({v = 1}, mt), setmetatable({v = 2}, mt)
        local cnt = 0
        for i = 1, 90 do
          if i == 30 then mt.__lt = function(a, b) return a.v < b.v end; mt.__le = function(a, b) return a.v <= b.v end end
          if i == 60 then mt.__eq = function(a, b) return true end end
          local ok, r3 = pcall(function() return x < y, x <= y, x == y end)
          if ok and r3 then cnt = cnt + 1 end
          if not ok then cnt = cnt + 100 end
        end
        out[#out + 1] = cnt
        -- __lt that yields, inside a coroutine, in a hot loop of string comparisons
        local co = coroutine.wrap(function()
          local n = 0
          local m2 = {__lt = function(a, b) coroutine.yield(n); return true end}
          local p, q = setmetatable({}, m2), setmetatable({}, m2)
          for i = 1, 20 do
            if "a" < "b" then n = n + 1 end
            if p < q then n = n + 1 end
          end
          return -n
        end)
        local last
        for i = 1, 21 do last = co() end
        out[#out + 1] = last
        return table.concat(out, ";")
    "#,
    );
}

/// C2: tier-1 `SETUPVAL` of collectable values (`jit_setupval`: the store plus the upvalue barrier): closed and open upvalues, strings,
/// tables, functions, nil/booleans mixed, a black closed upvalue that receives fresh objects while an incremental / generational
/// collection is in progress. Run with `RUSTYNAIL_GC_STRESS=2 RUSTYNAIL_GC_VERIFY=1` for the barrier check (mutant `c2setupval_barrier_skip`).
#[test]
fn c2_tier1_setupval() {
    same(
        r#"
        local function mk()
          local up = {0}
          return function(v) up = v end, function() return up end
        end
        local set, get = mk()
        local out = {}
        for mode = 1, 2 do
          collectgarbage(mode == 1 and "incremental" or "generational")
          for round = 1, 12 do
            collectgarbage("step", 0)
            for i = 1, 20 do
              set({i, round, {i * 2}})
              if i % 5 == 0 then set("str" .. i .. round) end
              if i % 7 == 0 then set(print) end
              if i % 11 == 0 then set(nil) end
              if i % 13 == 0 then set(i % 2 == 0) end
            end
            set({round, {round}, "x" .. round})
            collectgarbage("step", 0)
            collectgarbage("step", 0)
            local v = get()
            out[#out + 1] = type(v) == "table" and (v[1] + v[2][1] + #v[3]) or -1
          end
        end
        -- an open upvalue (the enclosing frame is still running) and a counter that holds strings
        local cur = "start"
        local function setcur(v) cur = v end
        for i = 1, 100 do setcur("v" .. i); if i % 10 == 0 then collectgarbage("step", 0) end end
        out[#out + 1] = cur
        -- an upvalue shared by two closures, written from a coroutine
        local shared = {}
        local function w(v) shared = v end
        local co = coroutine.wrap(function() for i = 1, 30 do w({i}); coroutine.yield(#shared) end end)
        for i = 1, 30 do out[#out + 1] = co() end
        out[#out + 1] = shared[1]
        return table.concat(out, ",")
    "#,
    );
}
