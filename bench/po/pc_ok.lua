local n = 4000000
local ok_count = 0
local function risky(i) if i % 1000000000 == 0 then error("bad " .. i) end return i end
for i = 1, n do
  local ok, v = pcall(risky, i)
  if ok then ok_count = ok_count + v end
end
print(ok_count)
