-- Section 77.7 (`retf` bit 3): side traces of a function-entry family pop the root frame when it returns into a frame of its own prototype
-- (a recursion returning into the caller's loop), then continue in the caller. Trees of varying shape, values that change type, early
-- returns, an error raised below the pop under pcall, a method recursion and a recursion that also returns into another prototype.

local function mk(depth, width, seed)
  if depth == 0 then
    if seed % 3 == 0 then return { kind = "str", value = "s" .. seed } end
    return { kind = "num", value = seed * 0.5 }
  end
  local items = {}
  for i = 1, width do items[i] = mk(depth - 1, width, seed * 7 + i) end
  return { kind = "arr", items = items }
end

local nodes, sum, slen
local function walk(v)
  nodes = nodes + 1
  local k = v.kind
  if k == "num" then
    sum = sum + v.value
  elseif k == "str" then
    slen = slen + #v.value
  else
    for i = 1, #v.items do walk(v.items[i]) end
  end
end

for r = 1, 6 do
  nodes, sum, slen = 0, 0, 0
  walk(mk(4, 3 + r % 2, r))
  print("walk", r, nodes, sum, slen)
end

-- a recursion with an early return and a result used by the caller's loop
local function depthsum(v, d)
  if v.kind ~= "arr" then return d end
  local s = 0
  for i = 1, #v.items do
    local x = depthsum(v.items[i], d + 1)
    if x > 1000 then return x end
    s = s + x
  end
  return s
end
for r = 1, 6 do print("depthsum", r, depthsum(mk(4, 3, r), 0)) end

-- an error below the pop, caught by pcall in the loop
local function check(v, lim)
  if v.kind == "num" and v.value > lim then error("too big " .. v.value) end
  if v.kind == "arr" then
    for i = 1, #v.items do check(v.items[i], lim) end
  end
  return true
end
for r = 1, 8 do
  local ok, e = pcall(check, mk(3, 3, r), 40 + r * 10)
  print("check", r, ok, ok and "" or (tostring(e):gsub("^.-:%d+: ", "")))
end

-- a method recursion, and a recursive function that is also called from another function's loop
local T = {}
T.__index = T
function T:count(v)
  if v.kind ~= "arr" then return 1 end
  local n = 1
  for i = 1, #v.items do n = n + self:count(v.items[i]) end
  return n
end
local t = setmetatable({}, T)
local function outer(trees)
  local s = 0
  for i = 1, #trees do s = s + t:count(trees[i]) end
  return s
end
local trees = {}
for i = 1, 6 do trees[i] = mk(3, 2 + i % 3, i) end
for r = 1, 5 do print("count", r, outer(trees)) end
