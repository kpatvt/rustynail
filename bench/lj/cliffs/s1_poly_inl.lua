-- S1: polymorphic branches inside small inlined functions called from a hot loop (guard exits inside inlined frames).
--   lua s1_poly_inl.lua   (N iterations)
local function sel(x)
  if x % 3 == 0 then return x + 1 elseif x % 3 == 1 then return x * 2 else return x - 1 end
end
local function clamp(v, lo, hi)
  if v < lo then return lo elseif v > hi then return hi end
  return v
end
local function step(i, s)
  local a = sel(i)
  local b = clamp(a - s % 7, 2, 40)
  return s + b
end
local n = tonumber(arg and arg[1]) or 6000000
local s = 0
for i = 1, n do s = step(i, s) end
print(s)
