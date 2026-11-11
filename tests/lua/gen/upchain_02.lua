local u0 = 0
local function f0(p0)
 local u1 = p0 + u0
 local function f1(p1)
  local u2 = p1 + u1
  return u0 + u2
 end
end
return f0
