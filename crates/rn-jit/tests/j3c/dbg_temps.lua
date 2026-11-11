-- values of "(temporary)" slots and "(C temporary)" slots seen by getlocal on frames running compiled code
local function temps(level)
  local acc = {}
  for k = 1, 30 do
    local nm, v = debug.getlocal(level + 1, k)
    if not nm then break end
    if nm:sub(1, 1) == "(" then
      acc[#acc + 1] = k .. nm:sub(2, 4) .. "=" .. (type(v) == "table" and "tbl" or tostring(v))
    end
  end
  return table.concat(acc, ",")
end
local function f(a, b)
  local t = {}
  local x = a * 2 + b
  t[1] = x
  local y = (a + b) * (a - b)
  P("t1", temps(1))
  local s = 0
  for i = 1, 3 do
    s = s + i * x
    local u = s // 3
    P("t2", i, temps(1))
  end
  local z = {a, b, x, y}
  P("t3", temps(1))
  local w = x .. "-" .. y
  P("t4", w, temps(1))
  return s
end
P(f(3, 4))
P(f(5.5, 2))
-- the same from inside a call chain: arguments being prepared for a call are temporaries of the caller
local function g(...) P("t5", temps(2)) return ... end
local function h(a)
  local r = g(a, a + 1, a * 2)
  local q = { g(a + 1) }
  return r
end
P(h(7))
return table.concat(out, "\n")
