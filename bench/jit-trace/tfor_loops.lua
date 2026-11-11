-- informational (task F3b): generic for over arrays (ipairs) and hash tables (pairs / next) in a hot loop
local N = tonumber(arg and arg[1]) or 20000
local arr, hash = {}, {}
for i = 1, 1000 do arr[i] = i; hash["k" .. i] = i end
local function f(n)
  local s = 0
  for r = 1, n do
    for i, v in ipairs(arr) do s = s + i + v end
    for k, v in pairs(hash) do s = s + v end
  end
  return s
end
print(f(N))
