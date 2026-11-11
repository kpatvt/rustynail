local keys = {}
for i = 1, 64 do keys[i] = "k" .. i .. "x" end
local t = {}
local N = tonumber(arg and arg[1]) or 20000000
for i = 0, N do
  local k = keys[i % 64 + 1]
  t[k] = (t[k] or 0) + 1
end
local s = 0
for _, v in pairs(t) do s = s + v end
print(s)
