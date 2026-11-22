-- E1 (up-recursion unrolling, docs/JIT_TRACE.md section 34): the stack overflow happens at the same depth with the same message, errors
-- raised deep inside an unrolled recursion carry the same message and traceback, pcall at every level.

-- overflow of a function whose unrolled entry trace pushes several frames per iteration: the depth reached does not depend on how warm
-- the traces are (the first, cold run and the warm ones must agree); the message is the interpreter's
local function deep(n) if n < 0 then return 0 end return deep(n + 1) + 1 end
local depths = {}
for r = 1, 4 do
  local d = 0
  local function f(n) d = d + 1 if n < 0 then return 0 end return f(n + 1) + 1 end
  local ok, e = pcall(f, 1)
  depths[r] = d
  if r == 1 then print("overflow", ok, (tostring(e):gsub("^[^:]*:%d+: ", "")):sub(1, 40)) end
end
print("same depth", depths[1] == depths[2], depths[2] == depths[3], depths[3] == depths[4])
local ok, e = pcall(deep, 1)
print("deep", ok, (tostring(e):gsub("^[^:]*:%d+: ", "")):sub(1, 40))

-- recursion close to the limit, entered again and again (the entry checks cover the room of every unrolled frame)
local function h(n, lim) if n >= lim then return n end return h(n + 1, lim) + 0 end
for _, lim in ipairs { 100, 1000, 30000, 100000 } do
  local ok2, r = pcall(h, 1, lim)
  print("lim", lim, ok2, ok2 and r or "overflow")
end

-- an error at a chosen depth: message and the frames of the traceback
local function boom(n, at) if n == at then error("boom at " .. n) end return boom(n + 1, at) + 1 end
for _, at in ipairs { 1, 2, 3, 4, 5, 9, 12 } do
  local ok3, msg = xpcall(boom, function(m)
    local tb = debug.traceback(m, 1)
    local frames = 0
    for _ in tb:gmatch("in upvalue 'boom'") do frames = frames + 1 end
    for _ in tb:gmatch("in function 'boom'") do frames = frames + 1 end
    for _ in tb:gmatch("in local 'boom'") do frames = frames + 1 end
    return (tostring(m):gsub("^[^:]*:%d+: ", "")) .. " frames=" .. frames
  end, 1, at)
  print("err", at, ok3, msg)
end

-- pcall at every level of a recursion, some levels failing
local function pc(n) if n == 0 then return 0 end
  local ok4, r = pcall(pc, n - 1)
  if n % 5 == 0 then error("lvl" .. n) end
  return (ok4 and r or 100) + 1
end
local acc = 0
for i = 1, 60 do local ok5, r = pcall(pc, i % 23) acc = acc + (ok5 and r or -1) end
print("pcall", acc)

-- an arithmetic error on a value that comes back from deep in the recursion
local function bad(n) if n == 0 then return nil end return bad(n - 1) end
local function use(n) return bad(n) + 1 end
for _, n in ipairs { 0, 1, 2, 3, 7 } do
  local ok6, m = pcall(use, n)
  print("arith", n, ok6, (tostring(m):gsub("^[^:]*:%d+: ", "")))
end

-- debug.getinfo seen from the deepest frame of an unrolled recursion
local function gi(n) if n == 0 then
    local names = {}
    for l = 1, 6 do local i = debug.getinfo(l, "nl") names[#names + 1] = (i and i.name or "-") .. ":" .. (i and i.currentline or 0) end
    return table.concat(names, " ")
  end
  local r = gi(n - 1)
  return r
end
for i = 1, 30 do gi(i % 6 + 3) end
print("getinfo", gi(5))
