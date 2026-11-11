local co = coroutine.wrap(function(...)
  local n = select("#", ...)
  local t = {coroutine.yield(...)}
  return n, #t, table.unpack(t)
end)
local args = {} for i = 1, 200 do args[i] = i end
local r = {co(table.unpack(args))}
print(#r, r[1], r[200])
r = {co(table.unpack(args, 1, 100))}
print(#r, r[1], r[2], r[102])
local deep = coroutine.wrap(function() local function f(n) if n == 0 then coroutine.yield("deep") return 0 end return 1 + f(n - 1) end return f(5000) end)
print(deep()) print(deep())
print(pcall(coroutine.wrap(function() local function f() return 1 + f() end return f() end)))
