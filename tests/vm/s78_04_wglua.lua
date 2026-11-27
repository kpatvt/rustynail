-- docs/JIT_TRACE.md section 78.3 (`wgloop`): a loop whose one call is a Lua function keeps the plain start-gate weight.
local function f(a) return a * 2 + 1 end
local acc = 0
for o = 1, 60 do
  for k = 1, 64 do acc = acc + f(k) end
end
print(acc)
