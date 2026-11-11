local floor = math.floor
local n = tonumber(arg and arg[1]) or 30000000
local function counter() local c = 0; return function(d) c = c + d; return c end end
local total = 0
for i = 1, floor(n / 10) do
  local f = counter()
  for j = 1, 10 do total = total + f(j) end
end
local function compose(f, g) return function(x) return f(g(x)) end end
local inc = function(x) return x + 1 end
local h = inc
for i = 1, 20 do h = compose(h, inc) end
for i = 1, floor(n / 20) do total = total + h(i) end
print(string.format("%d", total))
