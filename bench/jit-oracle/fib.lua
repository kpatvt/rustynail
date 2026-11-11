-- Call bucket: recursive calls (fib). The function receives itself as a parameter so that no upvalue or
-- global access is involved (calls only).
local function f(n, self)
  if n < 2 then return n end
  return self(n - 1, self) + self(n - 2, self)
end

local function setup(n)
  return n, f
end

if ORACLE then return f, setup end
print(f(setup(tonumber(arg and arg[1]) or 32)))
