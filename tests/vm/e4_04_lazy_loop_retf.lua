-- E4 (docs/JIT_TRACE.md section 35.1): a non-recursive callee with a loop, called from the leaves of a recursion whose own body has a loop.
-- The callee's side traces may pop their frame (RETF) and go on in the caller; they must never do so into a caller whose frames can be lazy
-- (the recursive function, once it is lazy-aware), and a caller they popped into first must never become lazy-aware.

local function loop(n)
  local s = 0
  for i = 1, n do s = s + i end
  return s
end
local function deep(d, n)
  if d == 0 then return loop(n) end
  local s = 0
  for i = 1, 1 do s = s + deep(d - 1, n) end
  return s + 1
end
local r = 0
for k = 1, 10 do r = r + deep(10, 100) end
for k = 1, 4 do r = r + deep(300, 100) end
for k = 1, 10 do r = r + deep(10, 100) end
print(r)
for k = 1, 10 do r = r + loop(100) end
print(r)

-- the same with the callee's loop length changing (more exits of the callee's loop trace, more side traces)
local function deep2(d, n)
  if d == 0 then return loop(n) end
  local s = 0
  for i = 1, 2 do s = s + deep2(d - 1, n + i) end
  return s
end
local r2 = 0
for k = 1, 20 do r2 = r2 + deep2(6, k % 7 + 1) end
print(r2)

-- a recursion whose first trace is its function-entry trace, then the callee is replaced by one that pops through an error
local function chk(n) if n > 50 then error("big " .. n) end return loop(n) end
local function deep3(d, n)
  if d == 0 then return chk(n) end
  local s = 0
  for i = 1, 1 do s = s + deep3(d - 1, n) end
  return s
end
local r3, errs = 0, 0
for k = 1, 60 do
  local ok, v = pcall(deep3, 8, k)
  if ok then r3 = r3 + v else errs = errs + 1 end
end
print(r3, errs)
