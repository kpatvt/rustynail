-- delete-the-cost: ok iterations call risky directly (a zero-cost traced pcall), error iterations keep the real pcall
local n = tonumber(arg and arg[1]) or 4000000
local ok_count, err_count = 0, 0
local function risky(i) if i % 3 == 0 then error("bad " .. i) end return i end
for i = 1, n do
  local ok, v
  if i % 3 ~= 0 then ok, v = true, risky(i) else ok, v = pcall(risky, i) end
  if ok then ok_count = ok_count + v else err_count = err_count + #v end
end
local function thrower() error({code = 1}) end
local c = 0
for i = 1, n // 4 do local ok, e = pcall(thrower); c = c + e.code end
local x = 0
for i = 1, n // 4 do
  local ok, v
  if i % 3 ~= 0 then ok, v = true, risky(i) else ok, v = xpcall(risky, function(m) return #m end, i) end
  x = x + (ok and v or 0)
end
print(ok_count, err_count, c, x)
