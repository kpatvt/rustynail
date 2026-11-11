-- review finding 1: stale mem_known at an inner-loop-head resume point.
-- `#t` returns a string now and then (the INT def guard fails), tier 1 stores the string, and the
-- inner-loop head is entered through its resume stub with `x` not re-guarded.
local calls = 0
local t = setmetatable({}, {__len = function()
  calls = calls + 1
  if calls % 7 == 0 then return "s" else return 5 end
end})
local function f(t, n)
  local s = 0
  for i = 1, n do
    local x = #t
    for j = 1, 3 do
      if j > 1 then x = j * 2; s = s + x end
    end
  end
  return s
end
local r = 0
for k = 1, 6 do r = r + f(t, 150) end
print(r)
