print("a" .. "b", "a" .. 1, 1 .. 2, 1.5 .. "x", "x" .. -1, 2^63 .. "", -0.0 .. "")
print("a" .. "b" .. "c" .. "d" .. "e", 1 .. 2 .. 3)
local s = "x"
print(s .. s .. s .. s, s .. 1 .. s, (s .. s) .. (s .. s))
print(#("a" .. string.rep("b", 100)), #(string.rep("ab", 50) .. string.rep("cd", 50)))
print("" .. "", "" .. "a", "a" .. "", ("" .. "") == "")
local e = ""
print(e .. e .. e .. "z" .. e)
print(1 .. "", 1.0 .. "", math.maxinteger .. "", math.mininteger .. "")
