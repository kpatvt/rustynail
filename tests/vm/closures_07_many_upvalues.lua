local a1,a2,a3,a4,a5,a6,a7,a8,a9,a10 = 1,2,3,4,5,6,7,8,9,10
local b1,b2,b3,b4,b5,b6,b7,b8,b9,b10 = 11,12,13,14,15,16,17,18,19,20
local function f()
  return a1+a2+a3+a4+a5+a6+a7+a8+a9+a10+b1+b2+b3+b4+b5+b6+b7+b8+b9+b10
end
print(f())
local function g()
  a1, a2, b1, b10 = 100, 200, 300, 400
  return function() return a1 + a2 + b1 + b10 end
end
print(g()(), f())
