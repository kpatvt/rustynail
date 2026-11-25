-- Section 59 B1 (`pureb`: string.byte ranges with one `j <= #s` guard, dead results dropped) and B2 (`opencall`: an open-result CALL
-- defines the registers from A up in the snapshot liveness): hot loops whose strings, ranges and callees change, with hooks reading locals.
local byte = string.byte

-- B1: ranges of 2..8 results, some results dead; strings get shorter than j, i > j, out of range, negative positions
local strs = { "abcdefghijklmnop", string.rep("xyz", 30), "short", "ab", "", "0123456789" }
local function ranges(s, n, w)
  local acc, cnt = 0, 0
  for i = 1, n do
    local a, b = byte(s, i, i + 1)
    if a then acc = acc + a else cnt = cnt + 1 end
    if b then acc = acc + b * 3 else cnt = cnt + 2 end
    local c = byte(s, i, i + w)             -- only the first result is used
    acc = acc + (c or 0)
    local p, q, r, t = byte(s, i, i + 3)
    acc = acc + (p or 0) + (r or 0) * 7    -- q, t dead
    local x, y, z = byte(s, i + 1, i)      -- i > j: no result
    acc = acc + (x or 0) + (y or 1) + (z or 2)
  end
  return acc, cnt
end
local outs = {}
for r = 1, 3 do
  for k = 1, #strs do
    for w = 1, 7 do
      local a, c = ranges(strs[k], 14, w)
      if r == 3 then outs[#outs + 1] = a .. ":" .. c end
    end
  end
end
print(table.concat(outs, " "))
-- the string shrinks under a hot trace (the `j <= #s` guard exits)
local function walk(list, w)
  local acc = 0
  for k = 1, #list do
    local s = list[k]
    local a, b, c, d = byte(s, 3, 3 + w)
    acc = acc + (a or -1) + (d or -2) * 5 + #s
  end
  return acc
end
local list = {}
for k = 1, 400 do list[k] = string.rep(string.char(65 + k % 26), 12) end
print(walk(list, 3), walk(list, 1), walk(list, 7))
for k = 300, 400, 7 do list[k] = string.rep("q", k % 6) end
list[399] = string.rep("w", 40)
print(walk(list, 3), walk(list, 1), walk(list, 7))
-- out-of-range and negative positions change under the trace
local acc = 0
local IS = { 1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, 3, 1, 2, -2, 0, 1, 2, 9, 10, 15, 2 }
for r = 1, 200 do
  for k = 1, #IS do
    local i = IS[k]
    local a, b, c = byte("abcdefghij", i, i + 2)
    acc = acc + (a or 0) + (b or 0) * 3 + (c or 0) * 5
    local e, f = byte("abcdefghij", -i, -i + 1)
    acc = acc + (e or 0) + (f or 0)
  end
end
print(acc)

-- B2: open calls (f(g(...)), return g(...), {g(...)}) with expression temporaries around them; the callee changes (exits at the CALL)
local function g1(x) return x, x + 1, x + 2 end
local function g2(x) return x * 2 end
local function f3(a, b, c) return a + (b or 0.5) + (c or 0.25) end
local function cnt(...) return select("#", ...) end
local function tail(g, x) return g(x) end
local function opencalls(n, flip)
  local s = 0.0
  local G = g1
  for i = 1, n do
    if i % flip == 0 then G = g2 elseif i % flip == 1 then G = g1 end
    s = s + (i * 2) + f3(G(i))
    s = s + (i * 0.5) * cnt(G(i)) + #{ G(i) }
    s = s + (tail(G, i)) - (i + 0.25) * (select(-1, G(i)))
  end
  return s
end
print(string.format("%.6f", opencalls(500, 97)), string.format("%.6f", opencalls(500, 5)), string.format("%.6f", opencalls(300, 2)))
-- the callee comes from a table (no branch decides it: the callee guard at the CALL exits) with a numeric temporary pending below it
local gs = { g1, g2, g1, g1, g2, g1, g1, g1, g2, g2, g1 }
local function pending(n)
  local s = 0.0
  for i = 1, n do
    local t = i * 0.5
    s = s + (t * 3) - f3(gs[i % #gs + 1](i))
    s = s + (t + 1) * cnt(gs[(i * 7) % #gs + 1](t))
  end
  return s
end
print(string.format("%.6f", pending(600)), string.format("%.6f", pending(301)))

-- B2: a line hook that reads the locals while the open-call loop runs (locals must be exact at every exit)
local seen, sum = 0, 0.0
local function hook(ev, line)
  local k = 1
  while true do
    local name, v = debug.getlocal(2, k)
    if not name then break end
    if name:sub(1, 1) ~= "(" and type(v) == "number" then
      seen = seen + 1
      sum = sum + v
    end
    k = k + 1
  end
end
local function hooked(n)
  local s = 0
  local G = g1
  for i = 1, n do
    if i % 13 == 0 then G = g2 else G = g1 end
    local before = i * 3
    s = s + before + f3(G(i))
    local after = s - i
    s = s + cnt(G(after)) + (before & 7)
  end
  return s
end
print(hooked(400))
local calls = 0
debug.sethook(function(ev, line) calls = calls + 1; if calls % 17 == 0 then hook(ev, line) end end, "l")
local hv = hooked(200)
debug.sethook()
print(hv, seen > 0, string.format("%.1f", sum))
print(hooked(400))
