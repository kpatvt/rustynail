-- K6: deep leaf-heavy recursion, errors thrown from inside, coroutines that yield inside the recursion
local function deep(n) if n == 0 then return 0 end return 1 + deep(n - 1) end
local s = 0
for i = 1, 60 do s = s + deep(i * 20) end
print("deep", s)
local function thrower(n, at) if n == at then error("boom" .. n) end if n == 0 then return 0 end return 1 + thrower(n - 1, at) end
local ok, bad = 0, 0
for i = 1, 300 do
  local st, v = pcall(thrower, i % 30 + 3, (i % 7 == 0) and 2 or -1)
  if st then ok = ok + v else bad = bad + #v end
end
print("err", ok, bad)
local function gen(n) coroutine.yield(n) if n == 0 then return 0 end return gen(n - 1) + 1 end
local total = 0
for i = 1, 60 do
  local co = coroutine.wrap(function() return gen(i % 11) end)
  for _ = 1, i % 11 + 2 do total = total + co() end
end
print("co", total)
local function so(n) if n == 0 then return 0 end return 1 + so(n - 1) end
local st, msg = pcall(so, 1e7)
print("so", st, type(msg))
