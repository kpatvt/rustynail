local u0 = 0
local function f0(p0)
 local u1 = p0 + u0
 local function f1(p1)
  local u2 = p1 + u1
  local function f2(p2)
   local u3 = p2 + u2
   local function f3(p3)
    local u4 = p3 + u3
    local function f4(p4)
     local u5 = p4 + u4
     return u0 + u5
    end
   end
  end
 end
end
return f0
