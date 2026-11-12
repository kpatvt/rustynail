
-- rawget / rawset of integer keys in the array part (a metatable that must not be called), hash-part keys, string keys,
-- float keys, a nil store that moves the border, and the length after the raw stores (5.4: the `alimit` hint).
local calls = 0
local mt = { __index = function(t, k) calls = calls + 1 return -1 end,
             __newindex = function(t, k, v) calls = calls + 1 end }
local t = setmetatable({ 1, 2, 3, 4, 5, 6, 7, 8 }, mt)
t.name = "x"
local rawget, rawset = rawget, rawset
local function f(n, base)
  local c = 0
  for i = 1, n do
    local k = (i + base) % 8 + 1
    rawset(t, k, i)
    c = c + rawget(t, k) + (rawget(t, "name") == "x" and 1 or 0)
    if rawget(t, k + 40) == nil then c = c + 1 end
  end
  return c
end
local r = 0
for j = 1, 40 do r = r + f(60, j) end
print(r, calls, #t, f(5, 3), rawget(t, 1.0), rawget(t, 9), rawget(t, 2^53))
-- nil stores and the border: rawset(t, k, nil) is not recorded, but runs between the recorded stores
local u = setmetatable({ 1, 2, 3, 4, 5, 6, 7, 8 }, mt)
local function g(n)
  local s = 0
  for i = 1, n do
    rawset(u, 8, i)
    s = s + #u + rawget(u, 8)
    if i % 17 == 0 then rawset(u, 8, nil) end
    if i % 17 == 1 then rawset(u, 8, i) end
  end
  return s
end
print(g(100), g(37), #u, calls)
-- collectable values and a type change in the middle
local function h(n)
  local a = setmetatable({ {}, {}, {}, {} }, mt)
  for i = 1, n do
    local k = i % 4 + 1
    rawset(a, k, i == 30 and "s" or (i == 45 and 1.5 or {}))
    if i % 9 == 0 then collectgarbage() end
  end
  return type(rawget(a, 1)) .. type(rawget(a, 2)) .. type(rawget(a, 3)) .. type(rawget(a, 4)), #a
end
print(h(60))
print(h(200))
