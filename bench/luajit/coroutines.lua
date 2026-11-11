local floor = math.floor
local n = tonumber(arg and arg[1]) or 5000000
-- reduced mod 2^32: the original sums wrap int64, which doubles cannot represent
local co = coroutine.wrap(function() local a = 0; while true do a = (a + coroutine.yield(a)) % 4294967296 end end)
co()
local s = 0
for i = 1, n do s = (s + co(i)) % 4294967296 end
local function gen(m) return coroutine.wrap(function() for i = 1, m do coroutine.yield(i) end end) end
local t = 0
for r = 1, 200 do for v in gen(floor(n / 200)) do t = t + v end end
local cnt = 0
for i = 1, floor(n / 20) do local c = coroutine.create(function(x) return x end); local _, v = coroutine.resume(c, i); cnt = cnt + v end
print(string.format("%d", s), string.format("%d", t), string.format("%d", cnt))
