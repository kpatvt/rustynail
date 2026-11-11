local fs = {}
for i = 1, 5 do local c = i fs[#fs + 1] = function() c = c + 1 return c end end
print(fs[1](), fs[1](), fs[2](), fs[5]())
local gs = {}
for _, v in ipairs({10, 20, 30}) do gs[#gs + 1] = function() return v end end
print(gs[1](), gs[2](), gs[3]())
local hs = {}
local j = 1
while j <= 3 do local k = j hs[j] = function() k = k * 2 return k end j = j + 1 end
print(hs[1](), hs[1](), hs[2](), hs[3]())
local rs = {}
local n = 0
repeat local m = n rs[#rs + 1] = function() return m end n = n + 1 until m >= 2
print(#rs, rs[1](), rs[2](), rs[3]())
local shared = {}
do local x = 0 shared.inc = function() x = x + 1 return x end shared.get = function() return x end end
shared.inc() shared.inc()
print(shared.get())
