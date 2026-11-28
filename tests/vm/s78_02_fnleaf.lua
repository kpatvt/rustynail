-- docs/JIT_TRACE.md section 78.2 (`fnleaf`): short leaf functions called from code that is not traced (the loop body makes a closure each
-- iteration) get function-entry roots of their own; a checking helper that reads `error` does not. Outputs against the interpreter.
local V = {}
V.__index = V
local function vec(x, y, z) return setmetatable({ x = x, y = y, z = z }, V) end
function V.dot(a, b) return a.x * b.x + a.y * b.y + a.z * b.z end
function V:len() return math.sqrt(self:dot(self)) end
local function clamp(v) if v < 0 then return 0 elseif v > 100 then return 100 end return v end
local function check(v) if v ~= v then error("nan") end return v end
local acc, n = 0, 0
for i = 1, 6000 do
  local f = function() return i end -- a closure per iteration: the loop itself is not traced
  local a = vec(i % 7, f() % 5, 3)
  acc = acc + clamp(a.x * 40 - 20) + check(a:dot(a))
  if i % 1000 == 0 then
    acc = acc + math.floor(a:len() * 100)
    n = n + 1
  end
  if i == 4000 then V.dot = function(p, q) return p.x * q.x end end -- the callee changes under the roots
end
print(acc, n)
local ok, e = pcall(check, 0 / 0)
print(ok, e)
