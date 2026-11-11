-- suspended coroutines holding compiled frames; the function (prototype) is only referenced by the coroutine
local N = SMALL and 20 or 60
local cos = {}
local function make(i)
  -- a fresh prototype per coroutine (load), compiled when the coroutine first runs
  local f = load("local id = ... return function(n) local s = 0 for k = 1, n do s = s + k * " .. i ..
                 " if k % 4 == 0 then s = s + coroutine.yield(s) end end return s end")(i)
  local co = coroutine.create(f)
  local ok, v = coroutine.resume(co, 20)
  return co, v
end
local first = 0
for i = 1, N do
  local co, v = make(i)
  cos[i] = co
  first = first + v
end
collectgarbage() collectgarbage()
collectgarbage("step", 100)
local total = 0
for round = 1, 6 do
  for i = 1, N do
    local ok, v = coroutine.resume(cos[i], round)
    total = total + (ok and v or -1000000)
  end
  if round % 2 == 0 then collectgarbage() end
end
P("resumed", first, total)
-- drop half of them while suspended, the rest run to the end
for i = 1, N, 2 do cos[i] = nil end
collectgarbage() collectgarbage()
local fin = 0
for i = 2, N, 2 do
  while true do
    local ok, v = coroutine.resume(cos[i], 1)
    if not ok then fin = fin - 1000 break end
    if coroutine.status(cos[i]) == "dead" then fin = fin + v break end
  end
end
P("finished", fin)
-- wrap-ed generators whose only reference to the compiled function is the coroutine
local gens = {}
for i = 1, 40 do
  gens[i] = coroutine.wrap(load("return function() for k = 1, 10 do coroutine.yield(k * " .. i .. ") end return -1 end")())
end
local gs = 0
for r = 1, 5 do for i = 1, 40 do gs = gs + gens[i]() end collectgarbage() end
P("gens", gs)
for r = 6, 11 do for i = 1, 40 do gs = gs + gens[i]() end collectgarbage() end
P("gens2", gs)
return table.concat(out, "\n")
