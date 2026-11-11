local x, y = 10, 2.5
print(x .. y, y .. x, x .. x, x .. "" .. y, "n=" .. x .. ",f=" .. y)
print(1 .. 2 == "12", 1 .. 2 .. 3 == "123", (1 .. 2) + 1)
print(-1 .. -2, 1e100 .. "", 0x10 .. "")
local n = 0
for i = 1, 5 do n = n .. i end
print(n)
print(#(2^53 .. ""), 2^53 .. "")
