-- slots copied by MOVE (two slots name one value), carried across the outer iteration, a table kept in a local, an unread local that is only overwritten
local function f(t, n)
  local a, unused = 0.5, 7.5
  local keep = t
  local x1, x2 = 1.0, 2.0
  for i = 1, n do
    local c1, c2 = x1, x2
    unused = c2
    x1 = x1 + 0.25
    for j = 1, 3 do
      a = a + c1 * j + c2
    end
    x2 = c1 + c2 * 0.5
    keep = keep
  end
  return a, x1, x2, unused, keep == t
end
local t = {}
for k = 1, 60 do f(t, 12) end
for _, n in ipairs { 12, 5, 1, 0 } do print(n, f(t, n)) end
