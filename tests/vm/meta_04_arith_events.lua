local mt = {}
for _, ev in ipairs({"add", "sub", "mul", "div", "mod", "pow", "idiv", "band", "bor", "bxor", "shl", "shr"}) do
  mt["__" .. ev] = function(a, b) return ev end
end
mt.__unm = function() return "unm" end
mt.__bnot = function() return "bnot" end
local o = setmetatable({}, mt)
print(o + 1, 1 + o, o - 1, o * 2, o / 2, o % 2, o ^ 2, o // 2)
print(o & 1, o | 1, o ~ 1, o << 1, o >> 1, 1 & o, 1 << o)
print(-o, ~o)
print(o + o, o + "x", "x" + o == nil)
local k = 5
print(o + k, k + o, o + 1.5, 1.5 + o, o // k)
print(pcall(function() return o < o end))
