-- informational (task F3): table constructors and field insertion in a hot loop (NEWTABLE, SETLIST, NEWREF, LEN on traces)
local N = tonumber(arg and arg[1]) or 3000000
local function f(n)
  local s = 0
  for i = 1, n do
    local t = { i, i + 1 }
    local o = { x = i }
    o.y = i * 2
    s = s + t[1] + t[2] + o.x + o.y + #t
  end
  return s
end
print(f(N))
