//! Lua snippets for the J2a templates (NEWTABLE, SETLIST, LEN, CONCAT, CLOSURE, POW and the table slow paths),
//! shared by `jit_alloc.rs` and `jit_alloc_stress.rs`. Each returns a string that must be identical with the JIT
//! off and on (`always`). No snippet allocates more than a few MB.
#![allow(dead_code)]

pub const PROGS: &[(&str, &str)] = &[
    (
        "newtable_and_setlist",
        r#"
        local out = {}
        for r = 1, 40 do
          local a = {}
          local b = {1, 2, 3}
          local c = {x = 1, y = 2, z = r}
          local d = {r, r + 1, n = r, r + 2}
          local e = {{r}, {r, r}, {{r}}}
          local big = {1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,
                       31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,47,48,49,50,51,52,53,54,55,56,57,58,59,60}
          local function mr() return r, r * 2, r * 3 end
          local m = {mr()}
          local m2 = {0, mr()}
          local m3 = {mr(), mr()}
          local m4 = {(mr())}
          out[#out + 1] = #a + #b + #c + #d + #e + #big + #m + #m2 + #m3 + #m4 + c.z + d.n + e[3][1][1] + big[60]
        end
        local src = "return {" .. string.rep("7,", 1500) .. "}"
        local huge = load(src)()
        out[#out + 1] = #huge .. ":" .. huge[1500]
        local src2 = "local x = ... return {x, " .. string.rep("x,", 1200) .. "x, f = x, g = x}"
        local h2 = load(src2)(5)
        out[#out + 1] = #h2 .. ":" .. h2[1202] .. h2.f .. h2.g
        local function va(...) return {...} end
        out[#out + 1] = #va(1, 2, 3) + #va() + #va(nil, nil, 3)
        local function va2(...) return {0, ...} end
        out[#out + 1] = #va2(1, 2, 3) + select('#', table.unpack(va2(4, 5)))
        return table.concat(out, ",")
    "#,
    ),
    (
        "newtable_errors_and_limits",
        r#"
        local out = {}
        local function try(f, ...) local ok, e = pcall(f, ...) out[#out + 1] = tostring(ok) .. ":" .. tostring(e) end
        for i = 1, 3 do
          try(function() local t = {}; t[0/0] = 1 end)
          try(function() local t = {}; t[nil] = 1 end)
          try(function() local t = {}; t[1 << 62] = 1; t[math.maxinteger] = 2; t[math.mininteger] = 3; return t[1 << 62] + t[math.maxinteger] + t[math.mininteger] end)
          try(function() return table.unpack({}, 1, 1 << 62) end)
          try(function() return table.concat({}, "", 1, 1 << 62) end)
          try(function() local t = table.pack(table.unpack({1, 2, 3})); return #t end)
          try(function() return #setmetatable({}, {__len = function() return 42 end}) end)
          try(function() local t = {} ; t[1.5] = 1; t[-1] = 2; t[0] = 3; return t[1.5] + t[-1] + t[0] end)
          try(function() local u = {} ; return rawlen(u), next(u) end)
        end
        return table.concat(out, "|")
    "#,
    ),
    (
        "len",
        r#"
        local out = {}
        local function try(f, ...) local ok, e = pcall(f, ...) out[#out + 1] = tostring(ok) .. ":" .. tostring(e) end
        for r = 1, 30 do
          local t = {}
          for i = 1, r do t[#t + 1] = i end
          local s = string.rep("ab", r)
          local u = {1, 2, nil, 4}
          local h = {n = 1}
          local m = setmetatable({}, {__len = function() return r * 100 end})
          local w = setmetatable({1, 2, 3}, {__index = function() return 1 end})
          out[#out + 1] = #t + #s + #u + #h + #m + #w + #"" + #"xyz"
          for i = 1, #t do t[i] = nil end
          out[#out + 1] = #t
        end
        try(function() local n = nil; return #n end)
        try(function() local n = 5; return #n end)
        try(function() local n = true; return #n end)
        try(function() local x = {} ; return #x.y end)
        try(function() return #setmetatable({}, {__len = function() error("boom") end}) end)
        return table.concat(out, ",")
    "#,
    ),
    (
        "concat",
        r#"
        local out = {}
        local function try(f, ...) local ok, e = pcall(f, ...) out[#out + 1] = tostring(ok) .. ":" .. tostring(e) end
        local mt = {__concat = function(a, b)
          local x = type(a) == "table" and "T" or tostring(a)
          local y = type(b) == "table" and "T" or tostring(b)
          return x .. "+" .. y end}
        for r = 1, 25 do
          local s, n, f = "str", r, r + 0.5
          out[#out + 1] = s .. n
          out[#out + 1] = n .. s
          out[#out + 1] = s .. n .. f .. s .. 1 .. 2.0
          out[#out + 1] = n .. f
          out[#out + 1] = (n .. "") .. (f .. "")
          out[#out + 1] = s .. s .. s .. s .. s .. s
          out[#out + 1] = n .. n .. n
          local o = setmetatable({}, mt)
          out[#out + 1] = o .. s
          out[#out + 1] = s .. o
          out[#out + 1] = s .. o .. n
          out[#out + 1] = o .. o
          out[#out + 1] = 1 .. o
          out[#out + 1] = (-0.0) .. "" .. (1/0) .. (-1/0) .. math.mininteger .. 2^63
          local lng = string.rep("x", 200)
          out[#out + 1] = #(lng .. lng .. n)
        end
        try(function() local a; return "x" .. a end)
        try(function() local a = {}; return "x" .. a end)
        try(function() local a = {}; return a .. "x" end)
        try(function() local a = true; return "x" .. "y" .. a .. "z" end)
        try(function() return 1 .. nil end)
        try(function() local t = {} return "k" .. t.name end)
        try(function() local t = {} return "k" .. t.name .. "j" end)
        try(function() return {} .. {} end)
        try(function() return setmetatable({}, {__concat = function() error("cc") end}) .. "x" end)
        return table.concat(out, "|")
    "#,
    ),
    (
        "closures",
        r#"
        local out = {}
        local fs = {}
        for i = 1, 20 do
          local x = i
          fs[#fs + 1] = function() x = x + 1 return x end
          fs[#fs + 1] = function() return x * 2 end
        end
        for r = 1, 3 do for i = 1, #fs do out[#out + 1] = fs[i]() end end
        local function counter()
          local c = 0
          return function() c = c + 1 return c end, function() return c end
        end
        local inc, get = counter()
        for i = 1, 30 do inc() end
        out[#out + 1] = get()
        local function outer(a)
          local b = a * 2
          local function mid(c)
            local function inner(d) a = a + 1; b = b + 1; return a + b + c + d end
            return inner
          end
          return mid, function() return a, b end
        end
        local mid, st = outer(3)
        local in1, in2 = mid(10), mid(20)
        out[#out + 1] = in1(1) + in2(2) + in1(3)
        local a, b = st()
        out[#out + 1] = a .. ":" .. b
        -- many closures, collected
        for i = 1, 300 do local k = {i} local f = function() return k[1] end out[#out] = f() end
        local up = {}
        for i = 1, 5 do local v = i up[i] = function() return v end end
        out[#out + 1] = tostring(debug.upvalueid(up[1], 1) == debug.upvalueid(up[1], 1))
        out[#out + 1] = tostring(debug.upvalueid(up[1], 1) ~= debug.upvalueid(up[2], 1))
        -- 255 upvalues compile; 256 do not (the loader's error is the same either way)
        local decl, use = {}, {}
        for i = 1, 255 do decl[#decl + 1] = "local v" .. i .. " = " .. i; use[#use + 1] = "v" .. i end
        local body = table.concat(decl, "\n") .. "\nreturn function() return " .. table.concat(use, "+") .. " end"
        local okf, f255 = pcall(load(body))
        out[#out + 1] = tostring(okf) .. ":" .. (okf and f255() or "")
        local decl2, use2 = {}, {}
        for i = 1, 256 do decl2[#decl2 + 1] = "local v" .. i .. " = " .. i; use2[#use2 + 1] = "v" .. i end
        local f2, err = load(table.concat(decl2, "\n") .. "\nreturn function() return " .. table.concat(use2, "+") .. " end")
        out[#out + 1] = tostring(f2) .. ":" .. tostring(err)
        return table.concat(out, ",")
    "#,
    ),
    (
        "pow",
        r#"
        local out = {}
        local function try(f, ...) local ok, e = pcall(f, ...) out[#out + 1] = tostring(ok) .. ":" .. tostring(e) end
        local vals = {0, 1, -1, 2, 3, 0.5, -0.5, 2^53, 1e308, -1e308, 1/0, -1/0, 0/0, 10, -8, 1/3, math.maxinteger, math.mininteger, -0.0}
        for r = 1, 3 do
          for i = 1, #vals do
            for j = 1, #vals do
              local x, y = vals[i], vals[j]
              out[#out + 1] = string.format("%.17g", x ^ y)
            end
            local x = vals[i]
            out[#out + 1] = string.format("%.17g %.17g %.17g %.17g", x ^ 2, x ^ 0.5, x ^ 3, x ^ -1)
          end
        end
        try(function() return "2" ^ 2 end)
        try(function() return 2 ^ "3" end)
        try(function() return "a" ^ 2 end)
        try(function() local t = {} return t ^ 2 end)
        try(function() local n return n ^ 2 end)
        try(function() return setmetatable({}, {__pow = function(a, b) return 42 end}) ^ 2 end)
        try(function() return 2 ^ setmetatable({}, {__pow = function(a, b) return 43 end}) end)
        local s = 0
        for i = 1, 200 do s = s + i ^ 2 + i ^ 0.5 + (i / 7) ^ 3 end
        out[#out + 1] = string.format("%.10g", s)
        return table.concat(out, ",")
    "#,
    ),
    (
        "table_slow_paths",
        r#"
        local out = {}
        -- append and fill
        local t = {}
        for i = 1, 500 do t[#t + 1] = i * 2 end
        local u = {}
        for i = 500, 1, -1 do u[i] = i end
        local h = {}
        for i = 1, 300 do h["k" .. i] = i end
        local o = {}
        for i = 1, 100 do o.a = i; o.b = i; o.c = i; o.d = i end
        local p = {}
        p.x = 1; p.y = 2; p.z = 3; p.w = 4; p.v = 5
        local g = {}
        for i = 1, 100 do g[i * 1.5] = i; g[-i] = i; g[i * 1000003] = i end
        local sp = {}
        for i = 1, 50 do sp[i * 7] = i end
        out[#out + 1] = #t .. ":" .. #u .. ":" .. o.a + o.d .. ":" .. p.v .. ":" .. g[1.5] .. g[-100] .. g[100 * 1000003] .. ":" .. sp[350]
        -- misses read nil
        local m = {}
        local cnt = 0
        for i = 1, 100 do if m[i] == nil then cnt = cnt + 1 end if m.name == nil then cnt = cnt + 1 end if m[i .. ""] == nil then cnt = cnt + 1 end end
        out[#out + 1] = cnt
        -- shrink and regrow
        local r = {}
        for round = 1, 5 do
          for i = 1, 100 do r[i] = i end
          for i = 1, 100 do r[i] = nil end
          for i = 1, 50 do r["s" .. i] = i end
          for i = 1, 50 do r["s" .. i] = nil end
        end
        out[#out + 1] = tostring(next(r) == nil)
        -- classes: __index tables, depth 3, SELF through the chain
        local A = {} A.__index = A
        function A:hello() return "A" .. self.id end
        function A:both() return "a" end
        local B = setmetatable({}, A) B.__index = B
        function B:both() return "b" end
        local C = setmetatable({}, B) C.__index = C
        local obj = setmetatable({id = 7}, C)
        for i = 1, 50 do out[#out + 1] = obj:hello() .. obj:both() .. tostring(obj.nothing) end
        -- __index function, __newindex function / table, rawset
        local log = {}
        local prox = setmetatable({}, {__index = function(_, k) log[#log + 1] = "g" .. tostring(k) return k end,
                                       __newindex = function(tt, k, v) log[#log + 1] = "s" .. tostring(k) rawset(tt, k, v) end})
        for i = 1, 5 do local v = prox[i] prox[i] = v prox["f" .. i] = i; local w = prox.zz end
        out[#out + 1] = table.concat(log, ";")
        local store = {}
        local fwd = setmetatable({}, {__newindex = store, __index = store})
        for i = 1, 20 do fwd[i] = i; fwd["k" .. i] = i end
        out[#out + 1] = #store .. ":" .. tostring(rawget(fwd, 1)) .. ":" .. fwd[5] .. fwd.k5
        -- strings
        local s = "Hello"
        for i = 1, 30 do out[#out + 1] = s:upper() .. s:sub(2, 3) .. s:len() .. (s):rep(2) end
        -- long chain: loop in __index
        local l1, l2 = {}, {}
        setmetatable(l1, {__index = l2}); setmetatable(l2, {__index = l1})
        local ok, e = pcall(function() return l1.x end)
        out[#out + 1] = tostring(ok) .. ":" .. tostring(e)
        local ok2, e2 = pcall(function() l1.y = 1 return rawget(l1, "y") end)
        out[#out + 1] = tostring(ok2) .. ":" .. tostring(e2)
        -- errors
        local function try(f, ...) local ok, e = pcall(f, ...) out[#out + 1] = tostring(ok) .. ":" .. tostring(e) end
        local nilv, numv, boolv = nil, 5, true
        for i = 1, 3 do
          try(function() return nilv.x end)
          try(function() return nilv[i] end)
          try(function() nilv.x = 1 end)
          try(function() nilv[i] = 1 end)
          try(function() return numv.x end)
          try(function() numv[1] = 2 end)
          try(function() return boolv.x.y end)
          try(function() local q = {} return q.a.b end)
          try(function() local q = {} q.a.b = 1 end)
          try(function() local q = {} return q:nomethod() end)
          try(function() return ("x"):nomethod() end)
          try(function() return nilv:foo() end)
          try(function() undefinedglobal.x = 1 end)
          try(function() local q = {} q[0/0] = 1 end)
          try(function() local q = {} q[nil] = 1 end)
          try(function() return rawset({}, 0/0, 1) end)
        end
        return table.concat(out, "|")
    "#,
    ),
    (
        "globals_and_upvalue_tables",
        r#"
        G1 = 0
        local out = {}
        for i = 1, 200 do G1 = G1 + i; if i % 10 == 0 then ("G" .. i):len() ; _G["dyn" .. i] = i end end
        for i = 10, 200, 10 do out[#out + 1] = _G["dyn" .. i] end
        local env = setmetatable({}, {__index = _G})
        local f = load("x = 1; y = (y or 0) + 1; return x + y + #tostring(z)", "chunk", "t", env)
        for i = 1, 20 do out[#out + 1] = f() end
        out[#out + 1] = rawget(env, "x") .. ":" .. tostring(rawget(_G, "x"))
        local lazy = setmetatable({}, {__newindex = function(t, k, v) rawset(t, k, v * 2) end})
        local f2 = load("a = 1; b = 2; a = 3; return a + b", "c2", "t", lazy)
        out[#out + 1] = f2() .. ":" .. lazy.a .. ":" .. lazy.b
        return G1 .. "|" .. table.concat(out, ",")
    "#,
    ),
    (
        "gc_pressure",
        r#"
        local out = {}
        for _, mode in ipairs{"incremental", "generational"} do
          collectgarbage(mode)
          local keep = {}
          for round = 1, 30 do
            local t = {}
            for i = 1, 100 do
              t[#t + 1] = {i, tostring(i), i .. "x"}
              t["k" .. i] = function() return i end
            end
            keep[round % 7 + 1] = t
            local s = ""
            for i = 1, 20 do s = s .. i .. "," end
            keep[round % 5 + 10] = s
            if round % 4 == 0 then collectgarbage("step", 2) end
            if round % 9 == 0 then collectgarbage() end
          end
          local sum = 0
          for k, v in pairs(keep) do if type(v) == "table" then sum = sum + #v + v.k5() else sum = sum + #v end end
          out[#out + 1] = sum
        end
        -- finalizers running during compiled allocations
        local fin = 0
        collectgarbage("stop")
        for i = 1, 50 do setmetatable({}, {__gc = function() fin = fin + 1 end}) end
        collectgarbage("restart")
        collectgarbage()
        for i = 1, 100 do local _ = {i} end
        collectgarbage()
        out[#out + 1] = fin
        return table.concat(out, ",")
    "#,
    ),
    (
        "coroutines_and_yields",
        r#"
        local out = {}
        local co = coroutine.wrap(function(n)
          local t = {}
          for i = 1, n do
            t[#t + 1] = i
            local s = "v" .. i
            coroutine.yield(s .. #t)
            local c = function() return i end
            t[#t + 1] = c() .. s
          end
          return "end" .. #t
        end)
        out[#out + 1] = co(5)
        for i = 1, 5 do out[#out + 1] = co() end
        -- metamethods that yield (the template exits and the interpreter handles them)
        local prox = setmetatable({}, {__index = function(_, k) return coroutine.yield(k) end,
                                       __newindex = function(_, k, v) coroutine.yield(k .. v) end,
                                       __concat = function(a, b) return coroutine.yield("cat") end,
                                       __len = function() return coroutine.yield("len") end})
        local co2 = coroutine.wrap(function()
          local a = prox.foo
          prox.bar = 3
          local c = prox .. "x"
          local d = #prox
          return tostring(a) .. tostring(c) .. tostring(d)
        end)
        out[#out + 1] = co2()
        out[#out + 1] = co2("A")
        out[#out + 1] = co2()
        out[#out + 1] = co2("C")
        out[#out + 1] = co2("D")
        return table.concat(out, ",")
    "#,
    ),
];
