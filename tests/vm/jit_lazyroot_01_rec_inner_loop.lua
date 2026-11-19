-- lazy region root: a self-recursive loop-free function whose callee has a loop, descended deep, unwound and entered again
-- (a stale region root of an earlier deep descent must never be used by a later entry or exit), also entered from C (pcall) and under errors
local function inner(n, x)
  local s = 0
  for i = 1, n do
    for j = 1, i % 3 + 1 do s = s + x end
    if i % 5 == 0 then s = s + 1 end
  end
  return s
end
local function rec(d, x)
  local s = inner(7, x)
  if d > 0 then s = s + rec(d - 1, x) end
  return s
end
local out = {}
for _, d in ipairs { 0, 1, 5, 50, 300, 1, 0, 120, 3 } do
  for k = 1, 3 do out[#out + 1] = rec(d, 1) end
end
out[#out + 1] = rec(100, 1.5)
print(table.concat(out, " "))
for k = 1, 6 do
  out[#out + 1] = select(2, pcall(rec, 40 + k, 2))
  out[#out + 1] = select(2, pcall(rec, 2, 2))
  local ok = pcall(rec, 30, {})
  out[#out + 1] = tostring(ok)
  out[#out + 1] = rec(k, 3)
end
print(table.concat(out, " "))
local co = coroutine.wrap(function(d)
  for k = 1, 5 do d = coroutine.yield(rec(d, 1)) end
  return rec(d, 1)
end)
local acc = {}
for k = 1, 6 do acc[#acc + 1] = co(k * 20) end
print(table.concat(acc, " "))
