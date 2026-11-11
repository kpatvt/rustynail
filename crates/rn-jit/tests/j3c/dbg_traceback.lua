local function rec(n, co)
  if n == 0 then
    if co then coroutine.yield("y") end
    return debug.traceback("tb", 1)
  end
  local r = rec(n - 1, co)
  return r
end
local function strip(s) return (s:gsub("0x%x+", "0xADDR")) end
P(strip(rec(5)))
-- inside a hot loop
local last
for i = 1, 40 do last = rec(3) end
P(strip(last))
-- coroutines: suspended mid compiled loop and mid recursion
local co = coroutine.create(function(a)
  local s = 0
  for i = 1, 30 do
    s = s + i
    if i % 10 == 0 then s = s + #rec(2, true) end
  end
  return s
end)
for i = 1, 4 do
  P("resume", coroutine.resume(co, 1))
  P(strip(debug.traceback(co)))
  P(strip(debug.traceback(co, "msg")))
  P(strip(debug.traceback(co, "msg", 1)))
  P(strip(debug.traceback(co, nil, 2)))
  local inf = debug.getinfo(co, 1, "Sl")
  P(inf and inf.currentline, inf and inf.what)
  if coroutine.status(co) == "suspended" then
    local nm, v = debug.getlocal(co, 1, 1)
    P("co local", nm, v)
  end
end
P("resume", coroutine.resume(co))
P("status", coroutine.status(co))
P(strip(debug.traceback(co)))
-- traceback of a coroutine that died with an error in a compiled frame
local co2 = coroutine.create(function()
  local t = {}
  for i = 1, 20 do t[i] = i end
  local x = t[5] + nil
end)
P(coroutine.resume(co2))
P(strip(debug.traceback(co2)))
P(strip(debug.traceback(co2, "m", 0)))
-- long chains are truncated the same way
local function deep(n) if n == 0 then return debug.traceback("deep") end local r = deep(n - 1) return r end
local tbk = strip(deep(40))
local _, nl = tbk:gsub("\n", "\n")
P("deep lines", nl, tbk:sub(1, 200))
-- xpcall handler traceback
P(xpcall(function() local t = nil; for i = 1, 3 do local _ = t.x end end, function(m) return strip(debug.traceback(m)) end))
P(type(debug.traceback({})))
return table.concat(out, "\n")
