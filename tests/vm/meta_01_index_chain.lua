local base = {foo = "basefoo", shared = 1}
local mid = setmetatable({bar = "midbar"}, {__index = base})
local top = setmetatable({baz = "topbaz"}, {__index = mid})
print(top.baz, top.bar, top.foo, top.shared, top.none)
print(rawget(top, "foo"), rawget(top, "baz"))
base.foo = "changed"
print(top.foo)
local fn = setmetatable({}, {__index = function(t, k) return tostring(k) .. "!" end})
print(fn.a, fn[1], fn[true], rawget(fn, "a"))
local deep = {}
for i = 1, 50 do deep = setmetatable({}, {__index = deep}) end
print(deep.x)
local root = {x = "found"}
local cur = root
for i = 1, 100 do cur = setmetatable({}, {__index = cur}) end
print(cur.x)
local self_ref = {}
setmetatable(self_ref, {__index = self_ref})
print(pcall(function() return self_ref.nothing end))
local a, b = {}, {}
setmetatable(a, {__index = b}) setmetatable(b, {__index = a})
print(pcall(function() return a.zzz end))
