local function f(...t) return t.n, t[1], t[2], t[3] end
print(f(), f(10), f(10, 20, 30))
local function g(a, ...t) return a, t.n, #t end
print(g(1), g(1, 2, 3))
local function h(...t) t[1] = "mod" t.n = 5 return t.n, t[1], select('#', ...) end
print(h(1, 2))
local function k(...t) return ... end
print(k(1, 2, 3))
local function m(...t) local u = {...} return #u, t.n, rawequal(t, u) end
print(m(1, 2, 3))
local function nested(...t)
  local function inner(...u) return u.n, t.n end
  return inner(t[1], t[2])
end
print(nested(5, 6, 7))
local function nilargs(...t) return t.n, t[1], t[2] end
print(nilargs(nil, nil))
