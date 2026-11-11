print(pcall(pcall, error, "e"))
print(pcall(pcall, pcall, error, "e"))
local ok, a, b = pcall(function()
  local ok2, e = pcall(error, "inner")
  error("outer:" .. e, 0)
end)
print(ok, a, b)
local function try(f, ...) local r = {pcall(f, ...)} return r[1], r[2] end
print(try(function(x) return x * 2 end, 21))
print(try(function(x) return x.y end, nil))
local depth = 0
local function nest(n) if n == 0 then error("bottom", 0) end local ok, e = pcall(nest, n - 1) depth = depth + 1 error(e .. ">" .. n, 0) end
print(pcall(nest, 5))
print(depth)
print(pcall(function() return pcall(error, "x") end))
