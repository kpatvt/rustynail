-- compiled code vs the collector: write barriers into old objects, mode switches inside compiled loops,
-- weak tables and finalizers observed from compiled frames
local modes = {"generational", "incremental"}
local function churn(n)
  local x
  for i = 1, n do x = {i, tostring(i), function() return i end} end
  return x
end
for round = 1, 4 do
  collectgarbage(modes[round % 2 + 1])
  collectgarbage() collectgarbage()           -- everything alive is old now (generational)
  local old = {}                              -- old table receiving young values from compiled stores
  local up                                    -- old upvalue
  local function setup(v) up = v end
  local objs = {}
  local sum = 0
  for i = 1, 400 do
    old[i] = {i, tostring(i)}                 -- SETTABLE with a young table into an old one
    old["k" .. i % 17] = {i}                  -- SETFIELD-style hash stores of young values
    setup({i})                                -- SETUPVAL of a young table into an old upvalue
    objs[#objs + 1] = function() return i + #old end   -- closures capturing, stored in old table
    if i % 37 == 0 then churn(50) end
    if i % 61 == 0 then collectgarbage("step", 1) end
    if i % 97 == 0 then collectgarbage(modes[(i // 97 + round) % 2 + 1]) end   -- mode switch in the loop
  end
  collectgarbage("step", 100)
  for i = 1, 400 do sum = sum + old[i][1] + #old[i][2] + objs[i]() end
  local keys = 0
  for k, v in pairs(old) do if type(k) == "string" then keys = keys + v[1] end end
  P("round", round, sum, keys, up[1], collectgarbage("incremental") ~= nil)
end
-- weak tables and finalizers seen from a compiled loop
do
  collectgarbage("stop")
  local weak = setmetatable({}, {__mode = "v"})
  local wk = setmetatable({}, {__mode = "k"})
  local order = {}
  local keep = {}
  for i = 1, 100 do
    local o = setmetatable({i}, {__gc = function(o) order[#order + 1] = o[1] end})
    weak[i] = o
    wk[o] = i
    if i % 10 == 0 then keep[#keep + 1] = o end
  end
  collectgarbage("restart")
  collectgarbage() collectgarbage()
  local alive, wkn = 0, 0
  for i = 1, 100 do if weak[i] then alive = alive + 1 end end
  for k in pairs(wk) do wkn = wkn + 1 end
  P("weak", alive, wkn, #order, #keep)
  table.sort(order)
  P("gc order head", order[1], order[2], order[#order])
end
-- finalizer resurrecting an object while a compiled loop runs
do
  local saved = {}
  local function mk(i) return setmetatable({i}, {__gc = function(o) saved[#saved + 1] = o end}) end
  for i = 1, 50 do mk(i) end
  local s = 0
  for i = 1, 100 do
    s = s + i
    if i % 10 == 0 then collectgarbage() end
  end
  collectgarbage() collectgarbage()
  local t = 0
  for _, o in ipairs(saved) do t = t + o[1] end
  P("resurrect", s, #saved, t)
  saved = nil
  collectgarbage() collectgarbage()
end
-- finalizer that errors and one that yields-attempt (warnings are off): compiled loop continues
do
  local n = 0
  for i = 1, 30 do
    setmetatable({}, {__gc = function() n = n + 1 error("gc error " .. i) end})
    if i % 7 == 0 then collectgarbage() end
  end
  collectgarbage() collectgarbage()
  P("gcerr", n)
end
-- finalizers that allocate heavily / run Lua loops (they run at GC points of the compiled caller)
do
  local total = 0
  local function mk(i) return setmetatable({}, {__gc = function() local t = {} for k = 1, 200 do t[k] = k * i end total = total + #t end}) end
  for i = 1, 60 do mk(i) end
  collectgarbage()
  for i = 1, 5000 do local t = {i} end
  collectgarbage() collectgarbage()
  P("heavy gc", total)
end
return table.concat(out, "\n")
