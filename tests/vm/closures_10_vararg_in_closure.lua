local function wrap(f, ...)
  local args = table.pack(...)
  return function(...) 
    local more = table.pack(...)
    local all = {}
    for i = 1, args.n do all[#all + 1] = args[i] end
    for i = 1, more.n do all[#all + 1] = more[i] end
    return f(table.unpack(all, 1, args.n + more.n))
  end
end
local add3 = wrap(function(a, b, c) return a + b + c end, 1, 2)
print(add3(3), add3(10))
local f = wrap(function(...) return select('#', ...), ... end, nil, nil)
print(f(nil))
