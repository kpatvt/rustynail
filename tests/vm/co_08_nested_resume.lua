local function chain(n)
  if n == 0 then return coroutine.yield("bottom") end
  local co = coroutine.wrap(chain)
  local r = co(n - 1)
  coroutine.yield(r .. n)
  return "done" .. n
end
local top = coroutine.wrap(chain)
print(top(5))
print(top())
local depth = 0
local function rec() depth = depth + 1 local c = coroutine.wrap(rec) return c() end
local okr, er = pcall(rec)
print(okr, (er:gsub("^.*: ", "")))
print(depth > 100)
