-- integer -> string conversion (tostring, concat, string.format %d, tostring in tables), every digit count and the limits
local vals = { 0, 1, -1, 9, 10, 99, 100, -100, 12345, math.maxinteger, math.mininteger, math.maxinteger - 1, math.mininteger + 1 }
local p = 1
for i = 1, 18 do p = p * 10; vals[#vals + 1] = p; vals[#vals + 1] = p - 1; vals[#vals + 1] = -p; vals[#vals + 1] = -p + 1; vals[#vals + 1] = p + 1 end
local out = {}
for _, v in ipairs(vals) do
  out[#out + 1] = tostring(v) .. "|" .. v .. "|" .. string.format("%d", v) .. "|" .. #tostring(v) .. "|" .. ("k" .. v .. "x")
end
print(table.concat(out, "\n"))
local t = {}
for i = 1, 300 do t[tostring(i * 7919)] = i end
local c = 0
for k, v in pairs(t) do c = c + tonumber(k) // v end
print(c, tostring(-0), tostring(0 // 1), 1 .. 2, -1 .. -2, math.mininteger .. "")
for i = 1, 200 do local s = "a" .. i; if #s ~= #tostring(i) + 1 then print("bad", i) end end
