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
     local function f5(p5)
      local u6 = p5 + u5
      local function f6(p6)
       local u7 = p6 + u6
       local function f7(p7)
        local u8 = p7 + u7
        local function f8(p8)
         local u9 = p8 + u8
         local function f9(p9)
          local u10 = p9 + u9
          return u0 + u10
         end
        end
       end
      end
     end
    end
   end
  end
 end
end
return f0
