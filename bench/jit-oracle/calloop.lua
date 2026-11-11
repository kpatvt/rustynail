-- Call bucket: a small Lua function called from a loop (the callee is passed as a parameter: no upvalue or
-- global access, calls only).
local function f(n, g)
  local s = 0
  for i = 1, n do
    s = s + g(i)
  end
  return s
end

local function g(x)
  return x + 1
end

local function setup(n)
  return n, g
end

if ORACLE then return f, g, setup end
print(f(setup(tonumber(arg and arg[1]) or 20000000)))
