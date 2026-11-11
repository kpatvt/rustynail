//! Lua snippets shared by `jit_calls.rs` and `jit_stress.rs`: each one returns a string (or number) that
//! must be identical with the JIT off and on. They cover the J1c protocol: flat calls and returns, natives,
//! errors through compiled frames, pcall, coroutines, varargs, tail calls, closures.
// (each test binary uses a different subset)
#![allow(dead_code)]

pub const PROGS: &[(&str, &str)] = &[
    (
        "calls_and_results",
        "local function z() end
         local function o() return 1 end
         local function t() return 1, 2, 3 end
         local function id(...) return ... end
         local r = {}
         local a, b, c = t() r[#r+1] = a .. b .. c
         local d, e = z() r[#r+1] = tostring(d) .. tostring(e)
         local f = o() r[#r+1] = f
         r[#r+1] = select('#', t())
         r[#r+1] = select('#', id(t(), t()))
         r[#r+1] = (t())
         local x, y, z2 = id(1, 2) r[#r+1] = tostring(x) .. tostring(y) .. tostring(z2)
         local tt = {t(), t()} r[#r+1] = #tt
         for i = 1, 50 do local p, q = t() r[#r+1] = p + q end
         return table.concat(r, ',')",
    ),
    (
        "fib",
        "local function fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
         return fib(18)",
    ),
    (
        "recursion_depth_and_stack_growth",
        "local function depth(n) if n == 0 then return 0 end return 1 + depth(n - 1) end
         local r = {}
         for _, n in ipairs{10, 100, 1000, 20000} do r[#r+1] = depth(n) end
         return table.concat(r, ',')",
    ),
    (
        "stack_overflow",
        "local function f(n) return 1 + f(n + 1) end
         local ok, e = pcall(f, 1)
         local function g(n) local a, b, c = g(n + 1) return a end
         local ok2, e2 = pcall(g, 1)
         return tostring(ok) .. ':' .. tostring(e):sub(1, 40) .. ':' .. tostring(ok2) .. ':' .. tostring(e2):sub(1, 40)",
    ),
    (
        "error_through_frames_traceback",
        "local function c() local t = nil; return t.x end
         local function b() local v = c() return v end
         local function a() local v = b() return v end
         local ok, tb = xpcall(a, debug.traceback)
         local ok2, e2 = pcall(function() local v = a() return v end)
         local ok3, e3 = pcall(error, 'plain', 2)
         local function lvl() error('lvl2', 2) end
         local function caller() lvl() end
         local ok4, e4 = pcall(caller)
         local ok5, e5 = pcall(function() local x = 1; x() end)
         local ok6, e6 = pcall(function() undefinedfn() end)
         return tb .. '|' .. e2 .. '|' .. tostring(e3) .. '|' .. e4 .. '|' .. e5 .. '|' .. e6",
    ),
    (
        "pcall_in_loops",
        "local function risky(i) if i % 3 == 0 then error('bad' .. i) end return i end
         local ok, bad, sum = 0, 0, 0
         for i = 1, 300 do
           local s, v = pcall(risky, i)
           if s then ok = ok + 1 sum = sum + v else bad = bad + 1 end
         end
         local s2, v2 = pcall(pcall, error, 'x')
         return ok .. ',' .. bad .. ',' .. sum .. ',' .. tostring(s2) .. tostring(v2)",
    ),
    (
        "coroutines",
        "local function leaf(i) coroutine.yield(i) return i * 2 end
         local function mid(i) local a = leaf(i) local b = leaf(a) return a + b end
         local co = coroutine.wrap(function(n)
           local s = 0
           for i = 1, n do s = s + mid(i) end
           return 'done' .. s
         end)
         local r = {}
         local v = co(3)
         while type(v) == 'number' do r[#r+1] = v v = co() end
         r[#r+1] = v
         local co2 = coroutine.create(function() local ok, e = pcall(function() coroutine.yield(1) error('in') end) coroutine.yield(e) return 'end' end)
         for i = 1, 4 do local ok, v2 = coroutine.resume(co2) r[#r+1] = tostring(v2):gsub('^.-:%d+: ', '') end
         return table.concat(r, ',')",
    ),
    (
        "natives",
        "local r = {}
         for i = 1, 40 do
           r[#r+1] = string.format('%d:%s', i, string.rep('ab', i % 4))
           r[#r+1] = math.floor(i / 3)
           r[#r+1] = select('#', table.unpack({1, 2, 3}, 1, i % 5))
           r[#r+1] = tostring(i):upper()
         end
         local t = {5, 2, 9, 1}
         table.sort(t, function(a, b) return a > b end)
         r[#r+1] = table.concat(t, '')
         for k, v in ipairs({10, 20, 30}) do r[#r+1] = k * v end
         return table.concat(r, ',')",
    ),
    (
        "varargs",
        "local function v(...) return select('#', ...), ... end
         local function w(a, b, ...) local n = select('#', ...) return a, b, n end
         local function pass(...) return v(...) end
         local r = {}
         r[#r+1] = table.concat({v()}, '/')
         r[#r+1] = table.concat({v(1, nil, 3)}, '/', 1, 4)
         r[#r+1] = table.concat({w(1)}, '/')
         r[#r+1] = table.concat({w(1, 2, 3, 4, 5)}, '/')
         r[#r+1] = table.concat({pass(7, 8)}, '/')
         for i = 1, 20 do r[#r+1] = select('#', pass(table.unpack({}, 1, i))) end
         return table.concat(r, ',')",
    ),
    (
        "methods_and_closures",
        "local Acc = {} Acc.__index = Acc
         function Acc.new(n) return setmetatable({n = n}, Acc) end
         function Acc:add(k) self.n = self.n + k return self end
         function Acc:get() return self.n end
         local a = Acc.new(1)
         for i = 1, 100 do a:add(i) end
         local function counter() local c = 0 return function() c = c + 1 return c end end
         local c1, c2 = counter(), counter()
         for i = 1, 10 do c1() end
         c2()
         local fs = {}
         for i = 1, 5 do fs[i] = function() return i * 10 end end
         local s = 0
         for i = 1, 5 do s = s + fs[i]() end
         return a:get() .. ',' .. c1() .. ',' .. c2() .. ',' .. s",
    ),
    (
        "tail_calls",
        "local function loop(n, acc) if n == 0 then return acc end return loop(n - 1, acc + n) end
         local function ev(n) if n == 0 then return true end return od(n - 1) end
         function od(n) if n == 0 then return false end return ev(n - 1) end
         local function nat(x) return tostring(x) end
         local function multi() return 1, 2, 3 end
         local function tm() return multi() end
         local function tn(...) return select('#', ...) end
         local function ts() return tn(tm()) end
         return loop(100000, 0) .. ',' .. tostring(ev(1001)) .. ',' .. nat(42) .. ',' .. ts() .. ',' .. select('#', tm())",
    ),
    (
        "call_metamethod_and_errors",
        "local obj = setmetatable({}, {__call = function(self, a, b) return a + b, self end})
         local r = {}
         for i = 1, 20 do r[#r+1] = (obj(i, 1)) end
         local ok, e = pcall(function() local n = nil; n() end)
         local ok2, e2 = pcall(function() return ({})() end)
         r[#r+1] = e r[#r+1] = e2
         return table.concat(r, ',')",
    ),
    (
        "generic_return",
        "local function two() return 1, 2 end
         local function three(a) return a, a + 1, a + 2 end
         local function none() return end
         local function va(...) return ... end
         local function mixed(a) return a, va(a, a) end
         local r = {}
         for i = 1, 30 do
           local a, b, c, d = three(i) r[#r+1] = a + b + c + (d or 0)
           local x, y = two() r[#r+1] = x + y
           r[#r+1] = select('#', none())
           r[#r+1] = select('#', mixed(i))
         end
         return table.concat(r, ',')",
    ),
    (
        "close_and_upvalues",
        "local log = {}
         local function f()
           local x <close> = setmetatable({}, {__close = function() log[#log+1] = 'closed' end})
           local up = 5
           local g = function() return up end
           return g() + 1
         end
         for i = 1, 3 do log[#log+1] = f() end
         local function h() local a = 1 local function inc() a = a + 1 end inc() inc() return a end
         log[#log+1] = h()
         return table.concat(log, ',')",
    ),
    (
        "gc_during_calls",
        "local keep = {}
         local function mk(i) return {i, tostring(i)} end
         for i = 1, 400 do
           keep[i % 17 + 1] = mk(i)
           if i % 50 == 0 then collectgarbage() end
         end
         local s = 0
         for _, v in pairs(keep) do s = s + v[1] end
         return s",
    ),
    (
        "hook_installed_mid_run",
        "local function f(x) return x + 1 end
         local n = 0
         for i = 1, 50 do n = n + f(i) end
         local c = 0
         debug.sethook(function() c = c + 1 end, '', 10)
         for i = 1, 100 do n = n + f(i) end
         debug.sethook()
         return n .. ',' .. c",
    ),
];

/// Programs whose hot functions use only instructions that have templates (MOVE, loads, FORLOOP, CALL,
/// RETURN*): everything they need besides that (natives, callees) is passed in as parameters, so the
/// calls really are made from compiled code.
pub const COMPILED_SHAPES: &[(&str, &str)] = &[
    (
        "natives_from_compiled_loop",
        "local function run(n, tostr, floor, rep, pc, sel, id, g)
           local a, b, c, d, e, f, h
           for i = 1, n do
             a = tostr(i)
             b = floor(i)
             c = rep(a, 2)
             d, e = pc(id, i)
             f = sel('#', i, i, i)
             h = g(id, i)
           end
           return a, b, c, d, e, f, h
         end
         local function id(x) return x end
         local function g(f, x) local y = f(x) return y end
         local r = {run(200, tostring, math.floor, string.rep, pcall, select, id, g)}
         for i = 1, 7 do r[i] = tostring(r[i]) end
         return table.concat(r, ',', 1, 7)",
    ),
    (
        "lua_chain_with_varied_results",
        "local function one(x) return x end
         local function two(x) return x, x + 0 end
         local function none(x) end
         local function va(...) return ... end
         local function run(n, one, two, none, va)
           local a, b, c, d, e
           for i = 1, n do
             a = one(i)
             a, b = two(i)
             none(i)
             c = none(i)
             d, e = va(i, a)
             c = va()
           end
           return a, b, c, d, e
         end
         local r = {run(300, one, two, none, va)}
         return table.concat({tostring(r[1]), tostring(r[2]), tostring(r[3]), tostring(r[4]), tostring(r[5])}, ',')",
    ),
    (
        "errors_in_compiled_callees",
        "local function boom(x) error('boom' .. tostring(x), 0) end
         local function run(n, boom, pc, id)
           local ok, e
           for i = 1, n do ok, e = pc(boom, i) id(ok) end
           return ok, e
         end
         local function lvl(f, x) local y = f(x) return y end
         local ok, e = run(50, boom, pcall, function(x) return x end)
         local ok2, e2 = pcall(lvl, boom, 7)
         local ok3, e3 = pcall(lvl, nil, 7)
         local ok4, e4 = pcall(lvl, {}, 7)
         return tostring(ok) .. e .. '|' .. e2 .. '|' .. e3 .. '|' .. e4",
    ),
    (
        "coroutines_across_compiled_frames",
        "local function leaf(y, i) local v = y(i) return v end
         local function mid(y, i) local v = leaf(y, i) local w = leaf(y, v) return w end
         local function body(y, n, mid)
           local s
           for i = 1, n do s = mid(y, i) end
           return s
         end
         local co = coroutine.wrap(function(n) return body(coroutine.yield, n, mid) end)
         local r = {}
         local v = co(5)
         for i = 1, 12 do r[#r+1] = tostring(v) v = co(i) end
         return table.concat(r, ',')",
    ),
    (
        "deep_chain_and_stack_growth",
        "local function down(self, n, id)
           local r
           if n then r = n end
           return r
         end
         local function step(f, x) local y = f(x) return y end
         local function run(n, step, id)
           local v
           for i = 1, n do
             v = step(step, id)
             v = step(id, i)
           end
           return v
         end
         local function id(x) return x end
         local ok, v = pcall(run, 1000, function(f, x) return x end, id)
         return tostring(ok) .. ',' .. tostring(v)",
    ),
    (
        "tailcall_from_compiled",
        "local function id(x) return x end
         local function tc(f, x) return f(x) end
         local function tn(f, x) return f(x, x) end
         local function run(n, tc, tn, id, tostr)
           local a, b
           for i = 1, n do a = tc(id, i) b = tn(tostr, i) end
           return a, b
         end
         local a, b = run(100, tc, tn, id, tostring)
         return tostring(a) .. ',' .. tostring(b)",
    ),
    (
        "returns_with_many_results",
        "local function r3(a) return a, a, a end
         local function rn(...) return ... end
         local function run(n, r3, rn)
           local a, b, c, d
           for i = 1, n do
             a, b, c, d = r3(i)
             a, b = rn(i, i + 0, i)
             a = rn()
             c, d = rn(a)
           end
           return a, b, c, d
         end
         local r = {run(100, r3, rn)}
         return tostring(r[1]) .. tostring(r[2]) .. tostring(r[3]) .. tostring(r[4])",
    ),
];

/// Deep recursion entirely in compiled frames (too slow for the stackmove stress: every new `CallInfo` takes the
/// helper path and would move a megabyte-sized stack).
pub const DEEP: &[(&str, &str)] = &[
    (
        "overflow_in_compiled_frames",
        "local function f(self, a, b) local x = self(self, a, b) return x end
         local ok, e = pcall(f, f, 1, 2)
         local ok2, e2 = pcall(f, f, 1, 2)
         return tostring(ok) .. tostring(e):gsub('^.-:%d+: ', '') .. tostring(ok2)",
    ),
    (
        "overflow_then_reuse",
        "local function f(self, a, b) local x = self(self, a, b) return x end
         local function id(x) return x end
         local function run(n, id) local s for i = 1, n do s = id(i) end return s end
         pcall(f, f, 1, 2)
         collectgarbage()
         return run(1000, id)",
    ),
];
