-- a callee returns and its registers stay in the frame above the caller's live registers; a later guard exits after
-- the return (popped-frame slots in the snapshot map)
local function g(a, b)
  local c = a + b
  local d = c * 3
  local e = d - a
  return e
end
local function f(n)
  local s, last = 0, 0
  for i = 1, n do
    local v = g(i, s % 7)
    s = s + v
    if i % 211 == 0 then last = i end
  end
  return s, last
end
for k = 1, 10 do print(f(900 + k)) end
