-- F4: coroutines yielding inside expressions with live temporaries; errors with pcall inside
local function producer(n)
  return coroutine.wrap(function()
    local s = 0.0
    for i = 1, n do
      s = s + (i * 1.5 + coroutine.yield(i * 0.5)) * (i + 2.25) - coroutine.yield(s) * 0.5
    end
    return s
  end)
end
local function drive(n)
  local p = producer(n)
  local acc = 0.0
  local v = p()
  local step = 0
  while step < 2 * n do
    step = step + 1
    acc = acc + v * 0.5
    v = p(step * 0.25 + 1)
  end
  return acc, v
end
local r = 0.0
for k = 1, 10 do r = r + (drive(30)) end
print(string.format("%.9g", r))
print(drive(25))
local co = coroutine.create(function(n)
  local s = 0.0
  for i = 1, n do
    local ok, e = pcall(function() return (i * 1.5 + (i % 7 == 0 and nil or i)) * 2.5 end)
    s = s + (ok and e or -1) * (i + 0.5) + (coroutine.yield(i) or 0)
  end
  return s
end)
local out = {}
local ok, v = coroutine.resume(co, 40)
while coroutine.status(co) == "suspended" do
  out[#out + 1] = v
  ok, v = coroutine.resume(co, 0.5)
end
print(ok, string.format("%.9g", v), #out)
