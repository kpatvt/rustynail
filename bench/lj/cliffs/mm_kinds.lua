-- N3: metamethods that are Lua closures called from a hot loop, one kind per run (the closures return numbers or booleans: no allocation).
--   lua mm_kinds.lua <add|addk|unm|len|concat|eq|lt|le|index|index_int|newindex|call>   (3M iterations)
local kind = arg and arg[1] or "add"
local n = 3000000
local V = {}
V.__add = function(a, b) return a.x + (type(b) == "table" and b.x or b) end
V.__unm = function(a) return -a.x end
V.__len = function(a) return a.x + 1 end
V.__concat = function(a, b) return a.x end
V.__eq = function(a, b) return a.x == b.x end
V.__lt = function(a, b) return a.x < b.x end
V.__le = function(a, b) return a.x <= b.x end
V.__call = function(self, k) return self.x + k end
local a, b = setmetatable({x = 1}, V), setmetatable({x = 2}, V)
local I = setmetatable({}, {__index = function(t, k) return 7 end})
local S = 0
local N = setmetatable({}, {__newindex = function(t, k, v) S = S + v end})
local s = 0
if kind == "add" then for i = 1, n do s = s + (a + b) end
elseif kind == "addk" then for i = 1, n do s = s + (a + 1) end
elseif kind == "unm" then for i = 1, n do s = s + (-a) end
elseif kind == "len" then for i = 1, n do s = s + #a end
elseif kind == "concat" then for i = 1, n do s = s + (a .. b) end
elseif kind == "eq" then for i = 1, n do if a == b then s = s + 1 else s = s + 2 end end
elseif kind == "lt" then for i = 1, n do if a < b then s = s + 1 end end
elseif kind == "le" then for i = 1, n do if a <= b then s = s + 1 end end
elseif kind == "index" then for i = 1, n do s = s + I.foo end
elseif kind == "index_int" then for i = 1, n do s = s + I[i] end
elseif kind == "newindex" then for i = 1, n do N.foo = i end s = S
elseif kind == "call" then for i = 1, n do s = s + a(i) end
end
print(s)
