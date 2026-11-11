local n = tonumber(arg and arg[1]) or 4000000
local ok_count, err_count = 0, 0
local function risky(i) if i % 3 == 0 then error("bad " .. i) end return i end
for i = 1, n do
  local ok, v = pcall(risky, i)
  if ok then ok_count = ok_count + v else err_count = err_count + #v end
end
local function thrower() error({code = 1}) end
local c = 0
for i = 1, n // 4 do local ok, e = pcall(thrower); c = c + e.code end
local x = 0
for i = 1, n // 4 do local ok, v = xpcall(risky, function(m) return #m end, i); x = x + (ok and v or 0) end
print(ok_count, err_count, c, x)
