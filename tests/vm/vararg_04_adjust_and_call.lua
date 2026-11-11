local function f(a, b, c) return a, b, c end
print(f(1), f(1, 2), f(1, 2, 3, 4))
local function g(...) return f(...) end
print(g(), g(9), g(9, 8, 7, 6))
local function h(...) local a, b = ... return a, b end
print(h(), h(1), h(1, 2, 3))
local function cnt(...) return select('#', ...) end
print(cnt(f()), cnt(f(), f()), cnt((f())), cnt(g(1, 2, 3, 4, 5)))
local function tail(...) return cnt(...) end
print(tail(), tail(nil), tail(1, 2, 3))
local big = {}
for i = 1, 200 do big[i] = i end
print(cnt(table.unpack(big)), (select(200, table.unpack(big))))
print(cnt(table.unpack(big, 1, 250)))
