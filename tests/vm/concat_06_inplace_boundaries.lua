-- concatenation results built in place: short/long boundary, empty operands, interning, identity
local function addr(s) return (string.format("%p", s)) end
local x40 = string.rep("x", 40)
local x41 = string.rep("x", 41)
-- 40 bytes is still a short (interned) string; 41 is long
print(#("x" .. string.rep("x", 39)), "x" .. string.rep("x", 39) == x40)
print(#(x40 .. "y"), x40 .. "y" == x41:sub(1, 40) .. "y")
local k = {}
k["ab" .. "cd"] = 1
k[string.rep("q", 20) .. string.rep("r", 20)] = 2
print(k.abcd, k[string.rep("q", 20) .. string.rep("r", 20)])
-- many short parts, some empty, mixed with numbers
local e = ""
print("a" .. e .. "b" .. e .. e .. "c" .. 1 .. e .. 2.5 .. "d")
print(e .. e, #(e .. e .. e), e .. 7, 7 .. e)
-- an empty operand returns the other operand unchanged (same object for long strings)
local long = string.rep("z", 100)
print(addr(long .. "") == addr(long), addr("" .. long) == addr(long))
print(addr(long .. "" .. "") == addr(long))
-- interned short results are the same object as the literal
local ab = "a" .. ("b" .. "")
print(addr(ab) == addr("ab"))
-- long results from many parts
local parts = {}
for i = 1, 50 do parts[i] = tostring(i) end
local s = parts[1]
for i = 2, 50 do s = s .. "," .. parts[i] end
print(#s, s == table.concat(parts, ","))
local a, b = string.rep("1", 30), string.rep("2", 30)
print(a .. b == string.rep("1", 30) .. string.rep("2", 30), #(a .. b .. a .. b))
-- strings with embedded zeros
print(#("a\0b" .. "\0c"), ("a\0b" .. "\0c") == "a\0b\0c")
-- metamethod in the middle of a chain of strings
local mt = {__concat = function(l, r)
  local lv = type(l) == "table" and "T" or l
  local rv = type(r) == "table" and "T" or r
  return lv .. rv
end}
local t = setmetatable({}, mt)
print("p" .. "q" .. t .. "r" .. "s")
print(t .. "" .. "u", "" .. t)
-- numbers next to a metamethod operand reach `__concat` unconverted
local c = setmetatable({}, {__concat = function(l, r) return type(l) .. "+" .. type(r) end})
print(c .. 5, 5 .. c, 4 .. c .. 5, 4 .. 5 .. c, c .. 4 .. 5, "s" .. c .. 1.5)
