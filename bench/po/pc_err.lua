local n = 4000000
local err_count = 0
local function risky(i) if i % 1 == 0 then error("bad " .. i) end return i end
for i = 1, n do
  local ok, v = pcall(risky, i)
  if not ok then err_count = err_count + #v end
end
print(err_count)
