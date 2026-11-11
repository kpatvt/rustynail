local t = {}
t[1] = "a" t[1.0] = "b" t[2^53] = "c"
print(t[1], t[1.0], #t, t[2^53|0])
t[2.5] = "x"
print(t[2.5], t[5/2])
local k = 3
t[k] = "i3" t[k + 0.0] = "f3"
print(t[3], t[3.0])
for _, key in ipairs({1, 2, 3}) do io.write(tostring(t[key]), " ") end
print()
print(next({}), rawlen({1, 2, 3}), rawequal(1, 1.0), rawequal("a", "a"))
local big = {[math.maxinteger] = 1, [math.mininteger] = 2, [0] = 3, [-1] = 4}
print(big[math.maxinteger], big[math.mininteger], big[0], big[-1], big[2^63])
