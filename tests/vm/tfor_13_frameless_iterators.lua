-- T2: generic-for over `next` / the `ipairs` iterator runs without a call frame in the interpreter
-- (tfor_fast); hooks, errors, metamethods, coroutines and tracebacks must look like the real call.
local out = {}
local function P(...)
  local t = {}
  for i = 1, select("#", ...) do t[i] = tostring((select(i, ...))) end
  out[#out + 1] = table.concat(t, " ")
end

-- basic: pairs / ipairs / explicit next, results counts 1..3
do
  local t = {10, 20, 30, x = 1}
  local n, s = 0, 0
  for k, v in pairs(t) do n = n + 1; s = s + v end
  P("pairs", n, s)
  local c = 0
  for k in next, t do c = c + 1 end
  P("next-keys", c)
  c = 0
  for i, v, extra in ipairs(t) do c = c + i * v; assert(extra == nil) end
  P("ipairs", c)
  c = 0
  for i in ipairs({}) do c = c + 1 end
  P("ipairs-empty", c)
  local a, b, d, e = 0
  for k, v, x, y in next, {5} do a, b, d, e = k, v, x, y end
  P("four-vars", a, b, tostring(d), tostring(e))
end

-- ipairs with __index (the fast path must give way) and holes
do
  local t = setmetatable({1, 2}, {__index = function(_, i) if i <= 5 then return i * 100 end end})
  local r = {}
  for i, v in ipairs(t) do r[#r + 1] = i .. "=" .. v end
  P("ipairs-index", table.concat(r, ","))
  local h = {1, 2, nil, 4}
  r = {}
  for i, v in ipairs(h) do r[#r + 1] = v end
  P("ipairs-hole", #r)
end

-- modification during traversal: assigning nil to existing fields, assigning existing fields
do
  local t = {}
  for i = 1, 50 do t["k" .. i] = i end
  local seen = 0
  for k, v in pairs(t) do
    t[k] = nil
    seen = seen + 1
  end
  P("clear-while-traversing", seen, next(t))
  local u = {a = 1, b = 2, c = 3}
  local sum = 0
  for k, v in pairs(u) do u[k] = v * 2; sum = sum + v end
  P("update-while-traversing", sum, u.a + u.b + u.c)
end

-- errors: invalid key to next, non-table state, nil iterator
do
  local ok, e = pcall(function() for k in next, {}, "nokey" do end end)
  P("next-invalid-key", tostring(ok), (tostring(e):gsub("^.-:%d+: ", "")))
  ok, e = pcall(function() for k in next, 42 do end end)
  P("next-non-table", tostring(ok), (tostring(e):gsub("^.-:%d+: ", "")))
  ok, e = pcall(function() for k in pairs(nil) do end end)
  P("pairs-nil", tostring(ok), (tostring(e):gsub("^.-:%d+: ", "")))
  ok, e = pcall(function() for i in ipairs(nil) do end end)
  P("ipairs-nil", tostring(ok), (tostring(e):gsub("^.-:%d+: ", "")))
  ok, e = pcall(function() for k in nil do end end)
  P("for-nil", tostring(ok), (tostring(e):gsub("^.-:%d+: ", "")))
end

-- __pairs and custom iterators (coroutine yield inside the iterator)
do
  local t = setmetatable({}, {__pairs = function(t)
    local i = 0
    return function() i = i + 1; if i <= 3 then return i, i * i end end, t, nil
  end})
  local r = {}
  for k, v in pairs(t) do r[#r + 1] = k .. ":" .. v end
  P("__pairs", table.concat(r, ","))
  local co = coroutine.wrap(function()
    local acc = {}
    for k in function(_, c) c = (c or 0) + 1; coroutine.yield(c); if c < 4 then return c end end, nil, nil do
      acc[#acc + 1] = k
    end
    return "done" .. #acc
  end)
  local ys = {}
  for i = 1, 5 do ys[#ys + 1] = tostring(co()) end
  P("yield-in-iterator", table.concat(ys, ","))
  -- a coroutine that yields from the loop body of a pairs loop
  local co2 = coroutine.wrap(function()
    for k, v in pairs({1, 2, 3}) do coroutine.yield(k + v) end
    return "end"
  end)
  P("yield-in-body", co2(), co2(), co2(), co2())
end

-- hooks: a call hook sees `next` (and the ipairs iterator) called by the loop, count hooks still count
do
  local calls = {}
  debug.sethook(function(ev)
    local info = debug.getinfo(2, "nS")
    if info.what == "C" then calls[#calls + 1] = tostring(info.name) end
  end, "c")
  for k in next, {1, 2} do end
  for i in ipairs({7, 8}) do end
  debug.sethook()
  P("call-hook", #calls, table.concat(calls, ","))
  local lines = 0
  debug.sethook(function() lines = lines + 1 end, "l")
  for k, v in pairs({1, 2, 3}) do
    local x = v
  end
  debug.sethook()
  P("line-hook", lines > 0)
  local cnt = 0
  debug.sethook(function() cnt = cnt + 1 end, "", 1)
  for k, v in pairs({1, 2, 3}) do end
  debug.sethook()
  P("count-hook", cnt > 3)
end

-- traceback inside the loop body does not show the iterator frame
do
  local tb
  for k in pairs({1}) do tb = debug.traceback("x", 1) end
  P("traceback", tb:match("^x\nstack traceback:\n") ~= nil, select(2, tb:gsub("\n", "\n")) >= 2)
end

-- to-be-closed variable in the 4th slot
do
  local closed = 0
  local tbc = setmetatable({}, {__close = function() closed = closed + 1 end})
  for k, v in next, {1, 2, 3}, nil, tbc do end
  local f, st, c0 = ipairs({1, 2, 3})
  for k, v in f, st, c0, tbc do break end
  P("tbc", closed)
end

-- a loop that grows the stack through deep recursion keeps working
do
  local function deep(n) if n == 0 then local c = 0; for _ in pairs({1, 2, 3}) do c = c + 1 end; return c end return deep(n - 1) end
  P("deep", deep(150))
end

print(table.concat(out, "\n"))
