-- J5 informational (task F4): tail-recursive loops inside a hot loop (TAILCALL replaces the frame; a 12-step tail loop,
-- mutual tail recursion and a tail-call chain with missing arguments). N = reps of a 1000-iteration loop.
local function loop(n, acc) if n == 0 then return acc end return loop(n - 1, acc + n) end
local ev, od
function ev(n) if n == 0 then return true end return od(n - 1) end
function od(n) if n == 0 then return false end return ev(n - 1) end
local function c(a, b, d) if d == nil then return a + b end return a + b + d end
local function b2(x) return c(x, 1) end
local function a2(x) local t = x + 1 return b2(t) end
local function run(n)
  local s = 0
  for i = 1, n do
    s = s + loop(12, 0) + a2(i)
    if ev(6 + i % 2) then s = s + 1 end
  end
  return s
end
local reps = tonumber(arg and arg[1]) or 6000
local total = 0
for r = 1, reps do total = total + run(1000) end
print(total)
