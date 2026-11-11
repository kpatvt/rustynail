local function mr() return 1, 2, 3 end
local t1 = {mr()}
local t2 = {mr(), mr()}
local t3 = {(mr())}
local t4 = {mr(), 10}
local t5 = {10, mr()}
print(#t1, #t2, #t3, #t4, #t5)
print(mr(), mr())
print((mr()), mr())
print(mr(), (mr()))
local a, b, c, d = mr()
print(a, b, c, d)
local x, y = (mr())
print(x, y)
local function wrap(...) return ... end
print(wrap(mr()), wrap(mr(), 4), wrap(wrap(wrap(mr()))))
print(#{mr(), mr(), mr()}, #{n = mr()})
return mr()
