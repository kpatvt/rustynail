local function gen(n)
  return coroutine.wrap(function()
    for i = 1, n do
      local x = i * 1.5
      coroutine.yield(x)
      x = x + 1
      coroutine.yield(x)
    end
  end)
end
local s = 0
for v in gen(60) do s = s + v end
print(s)
local co = coroutine.create(function(a)
  local acc = 0
  for i = 1, 40 do
    a = coroutine.yield(acc)
    acc = acc + (a or 0)
  end
  return acc
end)
local last
for i = 1, 41 do
  local ok, v = coroutine.resume(co, i % 2 == 0 and i or 7)
  last = v
end
print(last)
