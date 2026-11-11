local function f(...) return ... end
print(f(), f(1), f(1, 2, 3))
print((f(1, 2, 3)))
print(select('#'), select('#', nil), select('#', nil, nil), select('#', f()))
local function g(a, b, ...) return a, b, select('#', ...), ... end
print(g(), g(1), g(1, 2), g(1, 2, 3), g(1, 2, 3, 4))
print(select(2, "a", "b", "c"), select(-1, "a", "b", "c"), select(-2, "a", "b", "c"))
print(pcall(select, 0))
print(pcall(select, -5, 1))
print(select(5, 1, 2))
