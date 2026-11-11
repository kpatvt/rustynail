local out = {}
local function P(...) local t = table.pack(...); for i = 1, t.n do t[i] = tostring(t[i]) end; out[#out+1] = table.concat(t, ",") end
for _, seed in ipairs{0, 1, 42, -7, 123456789, math.maxinteger, math.mininteger} do
  local a, b = math.randomseed(seed)
  P("seed", a, b)
  for i = 1, 6 do P(math.random()) end
  for i = 1, 6 do P(math.random(10), math.random(-5, 5), math.random(0)) end
  P(math.random(math.mininteger, math.maxinteger), math.random(1 << 40), math.random(3, 3))
  P(pcall(math.random, 2, 1))
  P(pcall(math.random, 1, 2, 3))
  P(math.random(100))  -- state advanced after errors
  local a2, b2 = math.randomseed(seed, 99); P("seed2", a2, b2); P(math.random(1000), math.random())
end
math.randomseed()
for i = 1, 100000 do math.random(1, 6) end
math.randomseed(5); local s = 0; for i = 1, 100000 do s = s + math.random(1, 100) end; P("sum", s)
print(table.concat(out, "\n"))
