-- BUNDLE-K L1 (lazy CallInfo): observers inside native self-recursion. The recursion is warmed up first (so the entry / continuation /
-- base-case traces exist), then something looks at the call chain from a leaf at depth: traceback, getinfo, getlocal, error + pcall,
-- xpcall with a handler, a count hook, GC, and a coroutine yield. Every output must equal the reference interpreter's.
local function norm(s) return (tostring(s):gsub("0x%x+", "PTR")) end

-- the recursion under test (fib shape: two self-calls, base case n < 2)
local cnt, trigger, obs = 0, -1, nil
local function f(n)
  if n < 2 then
    cnt = cnt + 1
    if cnt == trigger then obs(n) end
    return n
  end
  return f(n - 1) + f(n - 2)
end
local function warm() for _ = 1, 30 do f(14) end cnt = 0 end

-- 1. traceback at a leaf, depth ~ n
local function tb() print(norm(debug.traceback("tb", 1))) end
warm() trigger = 40 obs = tb print(f(12)) trigger = -1

-- 2. getinfo of every level
local function gi()
  local lv = 1
  while true do
    local i = debug.getinfo(lv, "Slnt")
    if not i then break end
    io.write(lv, ":", i.currentline, ":", tostring(i.name), ":", i.what, ":", tostring(i.istailcall), " ")
    lv = lv + 1
  end
  print()
end
warm() trigger = 25 obs = gi print(f(11)) trigger = -1

-- 3. getlocal of the frames (parameter n of each level) and setlocal of the caller's
local function gl()
  local t = {}
  for lv = 2, 12 do
    local ok, name, v = pcall(debug.getlocal, lv, 1)
    if not ok or not name or name:sub(1, 1) == "(" then break end
    t[#t + 1] = name .. "=" .. norm(v)
  end
  print(table.concat(t, ","))
end
warm() trigger = 33 obs = gl print(f(10)) trigger = -1

-- 4. error from a leaf, caught outside, message and traceback through xpcall
local function boom(n) error("boom at " .. n, 0) end
warm() trigger = 50 obs = boom
print(pcall(f, 13))
warm() trigger = 50 obs = boom
print(xpcall(f, function(m) return norm(debug.traceback(m, 1)) end, 13))
trigger = -1

-- 5. error with a table value, rethrown
warm() trigger = 7 obs = function(n) error({ code = n }) end
local ok, e = pcall(f, 9) print(ok, type(e), e.code) trigger = -1

-- 6. count hook installed from the leaf (fires inside the recursion after it)
local hooks = 0
warm() trigger = 20 obs = function() debug.sethook(function() hooks = hooks + 1 end, "", 50) end
print(f(12)) debug.sethook() print(hooks > 0)
trigger = -1

-- 7. line hook installed from the leaf: counts lines of f
local lines = 0
warm() trigger = 12 obs = function() debug.sethook(function(ev, l) lines = lines + 1 end, "l") end
print(f(8)) debug.sethook() print(lines > 0, lines)
trigger = -1

-- 8. GC at depth: full collections and steps while the chain is deep, tables alive in the frames
local function g(n, t)
  if n == 0 then
    cnt = cnt + 1
    if cnt % 7 == 0 then collectgarbage() elseif cnt % 5 == 0 then collectgarbage("step", 0) end
    return #t
  end
  local a = { n }
  local x = g(n - 1, { a, t })
  local y = g(n - 1, { a })
  return x + y + a[1]
end
cnt = 0 for _ = 1, 20 do g(6, {}) end print(g(9, {}), cnt)

-- 9. a coroutine that yields from a leaf at depth and is resumed (the recursion is suspended and continues)
local co = coroutine.wrap(function()
  local function y(n)
    if n < 2 then
      cnt = cnt + 1
      if cnt % 11 == 0 then coroutine.yield(n) end
      return n
    end
    return y(n - 1) + y(n - 2)
  end
  return y(13)
end)
cnt = 0
local out = {}
while true do local v = co() out[#out + 1] = v if #out > 400 or v > 100 then break end end
print(#out, out[#out])

-- 10. pcall inside the recursion (protected calls at several depths, one fails)
local function h(n)
  if n < 2 then if n == 0 and cnt % 9 == 0 then error("leaf") end return n end
  local ok, v = pcall(h, n - 1)
  if not ok then v = -1 end
  return v + h(n - 2)
end
cnt = 0 for i = 1, 30 do cnt = cnt + 1 pcall(h, 10) end print(pcall(h, 12))
