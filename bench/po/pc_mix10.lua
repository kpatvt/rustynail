local n = 4000000
local ok_count, err_count = 0, 0
local function risky(i) if i % 10 == 0 then error("bad " .. i) end return i end
for i = 1, n do
  local ok, v = pcall(risky, i)
  if ok then ok_count = ok_count + v else err_count = err_count + #v end
end
print(ok_count, err_count)
