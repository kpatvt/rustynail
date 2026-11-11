local function gen(n) return coroutine.wrap(function() for i = 1, n do coroutine.yield(i, i * i) end end) end
for a, b in gen(5) do print(a, b) end
local function permgen(a, n)
  n = n or #a
  if n <= 1 then coroutine.yield(a) else
    for i = 1, n do a[n], a[i] = a[i], a[n]; permgen(a, n - 1); a[n], a[i] = a[i], a[n] end
  end
end
local count = 0
for p in coroutine.wrap(function() permgen({1,2,3,4}) end) do count = count + 1; io.write(table.concat(p, ""), " ") end
print(count)
local co = coroutine.create(function(...) return select("#", ...), ... end)
print(coroutine.resume(co, nil, nil, 3))
print(coroutine.resume(co))
