-- Integer -> string conversion (`number::tostring_int`, hand-written digit loop) at every digit
-- count and the extremes, and the empty-operand concatenation shortcut (`s .. ""`, `"" .. s`,
-- empty .. empty, numbers and `__concat` next to empty strings).
local t = {0, 1, -1, 9, 10, -10, 99, 100, 101, 999, 1000, 12345, -98765, math.maxinteger,
  math.mininteger, math.mininteger + 1, 1 << 62, -(1 << 53), 10 ^ 15 // 1, 9999999999, 10000000000}
for _, v in ipairs(t) do print(tostring(v), v .. "", #(v .. "x"), string.format("%d", v)) end
local o = {}
for i = -2000, 2000, 7 do o[#o + 1] = i .. "" end
print(table.concat(o, ","))
local p = 1
for _ = 1, 19 do print(p .. " " .. (p - 1) .. " " .. -p); p = p * 10 end

local e, s = "", "abc"
local long = string.rep("L", 100)
print(s .. e, e .. s, #(e .. e), e .. e == "", long .. e == long, e .. long == long)
print(1 .. e, e .. 2, 1.5 .. e, e .. -0.0, math.type(tonumber(7 .. e)))
local mt = {__concat = function(a, b)
  return "C(" .. tostring(type(a) == "table" and "T" or a) .. "," .. tostring(type(b) == "table" and "T" or b) .. ")"
end}
local obj = setmetatable({}, mt)
print(obj .. e, e .. obj, obj .. 1, 2 .. obj, e .. e .. obj, obj .. e .. e)
print(pcall(function() return e .. {} end))
print(pcall(function() return {} .. e end))
print(pcall(function() return e .. nil end))
-- identity of the result: the other operand itself
local k = string.rep("x", 50)
print(string.format("%p", k .. e) == string.format("%p", k), string.format("%p", e .. k) == string.format("%p", k))
