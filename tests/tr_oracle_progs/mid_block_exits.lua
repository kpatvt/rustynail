-- guards in the middle of basic blocks: array bounds, nil elements, a metatable appearing mid-run
local function f(t, n)
  local s = 0
  for i = 1, n do
    local x = t[i]
    local y = t[i + 1]
    s = s + (x or 0) + (y or 0)
  end
  return s
end
local t = {}
for i = 1, 600 do t[i] = i end
for rep = 1, 6 do print(f(t, 600 + rep)) end
t[300] = nil
print(f(t, 600))
setmetatable(t, {__index = function(_, k) return k % 3 end})
for rep = 1, 4 do print(f(t, 700)) end
