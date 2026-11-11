-- J5 F1 informational program `field_method` (task F1): method calls through SELF on a plain table whose methods are
-- fields (no metatable), callee inlined, the receiver's fields updated. N = reps (arg[1]); inner n = 1000.
local acc = { n = 0, total = 0 }
function acc.add(self, x) self.n = self.n + 1; self.total = self.total + x end
function acc.get(self) return self.total end
local function run(a, k)
  local s = 0
  for i = 1, k do
    a:add(i)
    s = s + a:get() % 7
  end
  return s
end
local reps = tonumber(arg and arg[1]) or 100000
local r = 0
for j = 1, reps do r = r + run(acc, 1000) end
print(r, acc.n, acc.total)
