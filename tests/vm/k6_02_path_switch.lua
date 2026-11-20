-- K6: a recursion whose leaf/non-leaf mix changes mid-run (leaf only, then deep, then leaf only again), mutual recursion, a callee replaced mid-run
local function f(n, k) if n <= k then return n end return f(n - 1, k) + f(n - 2, k) end
local out = {}
for round = 1, 12 do
  local k = (round % 4 == 0) and 0 or 100
  local s = 0
  for i = 1, 150 do s = s + f(i % 12 + 3, k) end
  out[#out + 1] = s
end
print(table.concat(out, " "))
local isodd
local function iseven(n) if n == 0 then return true end return isodd(n - 1) end
function isodd(n) if n == 0 then return false end return iseven(n - 1) end
local c = 0
for i = 1, 500 do if iseven(i % 9) then c = c + 1 end if isodd(i % 5) then c = c + 2 end end
print("eo", c)
local g
g = function(n) if n < 2 then return n end return g(n - 1) + g(n - 2) end
local g0, calls = g, 0
for i = 1, 200 do
  if i == 100 then g = function(n) calls = calls + 1 return g0(n) end end
  g(i % 10)
end
print("swap", calls)
