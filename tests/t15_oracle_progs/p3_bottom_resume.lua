-- review finding 3: bottom facts at pcs reached through resume points.
local function f(s, n, g, t)
  for i = 1, n do
    local x = s + 1     -- s is a string: ADDI is Dead for the region (string arithmetic is tier 1's)
    g()                 -- barrier: the resume stub after it re-enters
    local y = x
    t[i] = y
  end
end
local t = {}
f("10", 100, function() end, t)
print(#t, t[1], t[100])
-- POW: operands are not typed uses
local function h(s, n, g, t)
  for i = 1, n do
    g()
    local z = s ^ 2
    t[i] = z
  end
end
local t2 = {}
h("3", 100, function() end, t2)
h(3, 100, function() end, t2)
print(#t2, t2[1], t2[100])
