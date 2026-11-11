local function outer()
  local a = 1
  local function mid()
    local b = 10
    local function inner()
      a = a + 1
      b = b + 1
      return a, b
    end
    return inner
  end
  return mid(), function() return a end
end
local inner, geta = outer()
print(inner()) print(inner()) print(geta())
local i2 = select(1, outer())
print(i2())
local function chain(n)
  if n == 0 then return function() return 0 end end
  local f = chain(n - 1)
  return function() return f() + n end
end
print(chain(10)(), chain(50)())
