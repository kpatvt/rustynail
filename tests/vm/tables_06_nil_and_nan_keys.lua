local t = {}
print(pcall(function() t[nil] = 1 end))
print(pcall(function() t[0/0] = 1 end))
print(pcall(rawset, t, nil, 1))
print(t[nil], t[0/0], rawget(t, nil))
print(pcall(function() return {[nil] = 1} end))
print(pcall(function() return {[0/0] = 1} end))
local k = nil
print(pcall(function() t[k] = 1 end))
t[1e308 * 10] = "inf" t[-1e308 * 10] = "-inf"
print(t[math.huge], t[-math.huge])
t[2^53] = "big"
print(t[2^53], t[2^53 | 0], t[9007199254740992])
