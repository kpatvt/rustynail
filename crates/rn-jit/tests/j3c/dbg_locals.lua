local function dump(level)
  local k = 1
  local acc = {}
  while true do
    local nm, v = debug.getlocal(level + 1, k)
    if not nm then break end
    if nm:sub(1, 1) ~= "(" or nm == "(vararg)" or nm:sub(1, 5) == "(for " then
      acc[#acc + 1] = nm .. "=" .. tostring(v)
    else
      acc[#acc + 1] = nm
    end
    k = k + 1
  end
  return table.concat(acc, ",")
end
local function find(level, name)
  local k = 1
  while true do
    local nm = debug.getlocal(level + 1, k)
    if not nm then return nil end
    if nm == name then return k end
    k = k + 1
  end
end
-- setlocal of an ordinary register local inside a hot loop
local function f(n)
  local s = 0
  for i = 1, n do
    local a = i * 2
    local b = a + 1
    if i == 5 then
      P("at5", dump(1))
      P(debug.setlocal(1, find(1, "a"), 100))
      P(debug.setlocal(1, find(1, "b"), 7.5))
    end
    if i == 9 then P("at9", dump(1)) end
    s = s + a + b
  end
  return s
end
P("f", f(12))
-- setlocal of the visible control variable
local function g(n)
  local s = 0
  for i = 1, n do
    if i == 3 then debug.setlocal(1, find(1, "i"), 50) end
    s = s + i
  end
  return s
end
P("g", g(8))
-- setlocal of the loop's internal state (integer slots only): changes the iteration count
for _, nm in ipairs{"(for state)", "(for state)2", "(for state)3", "(for state)4"} do
  local function h(n)
    local c, s = 0, 0
    for i = 1, n do
      c = c + 1
      s = s + i
      if c == 4 then
        local k = find(1, nm)
        if k then P("set", nm, debug.getlocal(1, k)) debug.setlocal(1, k, 2) end
      end
      if c > 40 then break end
    end
    return c, s
  end
  P("h", nm, pcall(h, 10))
end
-- generic for state
local function gen(t)
  local c = 0
  for k, v in pairs(t) do
    c = c + 1
    if c == 2 then P("gen", dump(1)) end
  end
  return c
end
P("gen", gen{1, 2, 3, 4, 5})
-- tbc variables
local log = {}
local function mk(name)
  return setmetatable({}, {__close = function(_, e) log[#log + 1] = name .. ":" .. tostring(e) end})
end
local function tbc(n)
  local x <close> = mk("x")
  local y <close> = mk("y")
  for i = 1, n do
    local z <close> = mk("z" .. i)
    if i == 2 then
      P("tbcdump", dump(1))
      debug.setlocal(1, find(1, "y"), mk("y2"))
    end
  end
  return "done"
end
P("tbc", tbc(3), table.concat(log, ","))
-- varargs
local function va(...)
  local s = 0
  for i = 1, select("#", ...) do s = s + (select(i, ...)) end
  P("va", debug.getlocal(1, -1), debug.getlocal(1, -2), debug.getlocal(1, -4), debug.getlocal(1, -5))
  P("va set", debug.setlocal(1, -2, 1000))
  return s + select(2, ...)
end
P("va", va(1, 2, 3, 4))
-- parameters and locals of a caller's frame from a callee (level 2), then reading back
local function callee(k)
  local nm, v = debug.getlocal(2, k)
  debug.setlocal(2, k, (type(v) == "number" and v + 1000) or v)
  return nm
end
local function caller(a, b)
  local c = a + b
  local acc = 0
  for i = 1, 5 do
    callee(1) callee(3)
    acc = acc + a + c
  end
  return a, b, c, acc
end
P("caller", caller(1, 2))
-- getlocal on function values (parameter names only)
P(debug.getlocal(caller, 1), debug.getlocal(caller, 2), debug.getlocal(caller, 3))
-- error cases
P(pcall(debug.getlocal, 100, 1))
P(pcall(debug.setlocal, 100, 1, 1))
return table.concat(out, "\n")
