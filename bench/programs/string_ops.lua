local n = tonumber(arg and arg[1]) or 400000
local acc = 0
local parts = {}
for i = 1, n do
  local s = string.format("%d:%s:%5.2f", i, "item", i / 3)
  parts[#parts + 1] = s
  acc = acc + #s + #s:sub(2, -2) + (s:find(":", 1, true) or 0)
end
local big = table.concat(parts, ",")
local cnt = 0
for w in big:gmatch("%d+:item") do cnt = cnt + 1 end
local r, k = big:gsub("item", "ITEM")
local up = big:upper():lower()
local rep = ("abc"):rep(100000)
local x = 0
for i = 1, 200 do x = x + #(rep .. i) end
local s2 = ""
for i = 1, 20000 do s2 = s2 .. "x" end
print(acc, cnt, k, #up, #rep, x, #s2, big:byte(1, 3))
