local t = {}
t[1] = "a" t[2] = "b" t.x = "X" t["y"] = "Y" t[3.0] = "c" t[-1] = "neg" t[0] = "zero"
print(t[1], t[2], t.x, t.y, t[3], t[-1], t[0], #t)
local k = "x"
print(t[k], t["x"], t.x, rawget(t, "x"))
t[k] = nil
print(t.x, next({}) == nil)
local i = 1
t[i], t[i + 1] = t[i + 1], t[i]
print(t[1], t[2])
local function idx() return "x" end
t[idx()] = 5
print(t.x)
t.a, t.b, t.c = 1, 2
print(t.a, t.b, t.c)
local u = {}
u[1], u[2], u[3] = 3, 2, 1
print(u[1], u[2], u[3])
local o = {inner = {}}
o.inner.v = 7
o["inner"]["w"] = 8
print(o.inner.v, o.inner.w)
