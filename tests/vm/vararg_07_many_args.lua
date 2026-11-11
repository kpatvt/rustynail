local function cnt(...) return select('#', ...) end
local t = {}
for i = 1, 240 do t[i] = i end
print(cnt(table.unpack(t)))
local function sum(...)
  local s = 0
  for i = 1, select('#', ...) do s = s + (select(i, ...)) end
  return s
end
print(sum(table.unpack(t)))
local function f(...) return ... end
local r = {f(table.unpack(t))}
print(#r, r[1], r[240])
print(cnt(f(f(table.unpack(t, 1, 100)), f(table.unpack(t, 1, 100)))))
local function deep(n, ...) if n == 0 then return select('#', ...) end return deep(n - 1, n, ...) end
print(deep(100), deep(190))
