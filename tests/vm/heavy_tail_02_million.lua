local function count(n) if n == 0 then return "end" end return count(n - 1) end
print(count(1000000))
local function sum(n, acc) if n == 0 then return acc end return sum(n - 1, acc + n) end
print(sum(1000000, 0))
local function viapcall(n) if n == 0 then return "p" end return select(2, pcall(viapcall, n - 1)) end
print(viapcall(100))
