local live = {}
for i = 1, 200 do
  local t = {i}
  local f = function() return t[1] end
  if i % 50 == 0 then live[#live + 1] = f end
end
collectgarbage()
for _, f in ipairs(live) do io.write(f(), " ") end
print()
local function gen(n) local s = {} for i = 1, n do s[i] = function() return i end end return s end
local g = gen(100)
collectgarbage()
print(g[1](), g[50](), g[100]())
