local function pack(...) return {...}, select('#', ...) end
local t, n = pack(1, nil, 3, nil)
print(#t >= 1, n, t[1], t[2], t[3], t[4])
local function pk(...) return {n = select('#', ...), ...} end
local u = pk(nil, nil)
print(u.n, u[1], u[2], #u)
local function count(...) local c = 0 for _ in pairs({...}) do c = c + 1 end return c end
print(count(), count(1), count(1, 2, 3), count(nil, nil, 1))
print(table.unpack({1, 2, 3}))
print(table.unpack({1, 2, 3}, 2))
print(table.unpack({1, 2, 3}, 2, 3))
print(table.unpack({}, 1, 3))
local p = table.pack(10, 20, nil)
print(p.n, p[1], p[2], p[3])
