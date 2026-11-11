-- many coroutines running compiled functions
local N = SMALL and 40 or 300
local function body(id)
  local s = 0
  for i = 1, 12 do
    s = s + i * id
    if i % 3 == 0 then
      local ok, v = pcall(function()
        local got = coroutine.yield(s)             -- yield across pcall
        if got == "err" then error("e" .. id) end
        return got
      end)
      s = s + (ok and (v or 0) or #v)
    end
    if i % 4 == 0 then coroutine.yield(-s) end
  end
  return "done", s
end
local cos = {}
for i = 1, N do cos[i] = coroutine.create(body) end
local sum, steps, errs = 0, 0, 0
local alive = N
local round = 0
while alive > 0 do
  round = round + 1
  for i = 1, N do
    local co = cos[i]
    if co and coroutine.status(co) == "suspended" then
      local ok, a, b = coroutine.resume(co, i, (round + i) % 7 == 0 and "err" or round)
      steps = steps + 1
      if not ok then errs = errs + 1 elseif a == "done" then sum = sum + b alive = alive - 1 cos[i] = false
      else sum = sum + (type(a) == "number" and a or 0) end
    end
  end
  if round % 5 == 0 then collectgarbage() end
end
P("many", sum, steps, errs, round)
-- wrap generators, nested resumes
local function gen(n, depth)
  return coroutine.wrap(function()
    for i = 1, n do
      if depth > 0 then
        for v in gen(2, depth - 1) do coroutine.yield(v + i) end
      else
        coroutine.yield(i)
      end
    end
  end)
end
local tot = 0
for rep = 1, 20 do for v in gen(5, 3) do tot = tot + v end end
P("nested", tot)
-- close of suspended coroutines with pending tbc, from inside a hot loop
local log = {}
local function closer(name, fail)
  return setmetatable({}, {__close = function(_, e)
    log[#log + 1] = name .. ":" .. tostring(e)
    if fail then error("close-" .. name, 0) end
  end})
end
local function tbc_body(id)
  local a <close> = closer("a" .. id)
  local b <close> = closer("b" .. id, id % 4 == 0)
  local acc = 0
  for i = 1, 50 do
    local c <close> = closer("c" .. id .. "." .. i)
    acc = acc + i
    if i == 3 + id % 3 then coroutine.yield(acc) end
  end
  return acc
end
local closed = {}
for id = 1, 40 do
  local co = coroutine.create(tbc_body)
  coroutine.resume(co, id)
  local ok, e = coroutine.close(co)
  closed[#closed + 1] = tostring(ok) .. ":" .. tostring(e) .. ":" .. coroutine.status(co)
end
P("closed", table.concat(closed, ","))
P("log", #log, table.concat(log, ",", 1, 12))
-- coroutine.close of a running neighbour inside a loop; close of dead/normal coroutines
local co2
co2 = coroutine.create(function()
  local c = 0
  for i = 1, 30 do
    c = c + i
    if i == 10 then
      local ok, e = pcall(coroutine.close, co2)       -- closing the running coroutine
      P("self close", ok, (tostring(e):gsub("0x%x+", "A")))
    end
    if i == 20 then coroutine.yield(c) end
  end
  return c
end)
P(coroutine.resume(co2))
P(coroutine.close(co2), coroutine.status(co2))
P(coroutine.close(co2))
local outer
outer = coroutine.wrap(function()
  local inner = coroutine.create(function() coroutine.yield(1) end)
  coroutine.resume(inner)
  for i = 1, 10 do
    if i == 5 then P("close inner", coroutine.close(inner)) end
  end
  return "o"
end)
P(outer())
-- errors inside coroutine, resume of dead, wrap rethrow with position
local bad = coroutine.wrap(function() for i = 1, 10 do if i == 6 then error("wrapped " .. i) end end end)
P(pcall(bad))
P(pcall(bad))
-- isyieldable / running inside compiled loops
local co3 = coroutine.wrap(function()
  local r = {}
  for i = 1, 5 do
    local _, main = coroutine.running()
    r[#r + 1] = tostring(coroutine.isyieldable()) .. tostring(main)
    coroutine.yield()
  end
  return table.concat(r, ",")
end)
for i = 1, 5 do co3() end
P(co3())
return table.concat(out, "\n")
