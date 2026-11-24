-- docs/JIT_TRACE.md section 44.2 (T8, `libcall` bit 10): `string.find` with a real pattern recorded as the FINDP library op (start and end of a match without
-- captures). Same output as the reference in every mode.
local find, sub, format = string.find, string.sub, string.format

-- (1) a lexer shaped loop: anchored classes at a moving position, start and end used
local src = "local x = 10 + foo(bar, 'str') -- comment\nreturn x * 2.5e3 .. \"s\""
local function lex(s)
  local out, pos, n = {}, 1, #s
  while pos <= n do
    local a, b = find(s, "^%s+", pos)
    if a then
      pos = b + 1
    else
      a, b = find(s, "^[%a_][%w_]*", pos)
      if a then
        out[#out + 1] = sub(s, a, b)
        pos = b + 1
      else
        a, b = find(s, "^%d+%.?%d*[eE]?%d*", pos)
        if a then
          out[#out + 1] = sub(s, a, b)
          pos = b + 1
        else
          out[#out + 1] = sub(s, pos, pos)
          pos = pos + 1
        end
      end
    end
  end
  return out
end
local total = 0
for r = 1, 200 do total = total + #lex(src) end
print("lex", total, table.concat(lex(src), "|"))

-- (2) only the start is used (one result wanted), the outcome flips, the end is used in the next statement
local words = { "alpha", "beta12", "  gamma", "d3lta", "", "!!", "x" }
local hits, ends = 0, 0
for r = 1, 300 do
  local w = words[r % #words + 1]
  local a = find(w, "%d+")
  if a then hits = hits + a end
  local b, e = find(w, "^%s*%a+")
  if b then ends = ends + e - b end
end
print("one", hits, ends)

-- (3) init: negative, zero, past the end, a float with an integral value
local s = "hello world, hello lua"
for r = 1, 50 do
  local a, b = find(s, "l+", r - 25)
  local c, d = find(s, "o%s", -8)
  local e = find(s, "x*", #s + 1)
  local f, g = find(s, "x*", #s + 2)
  if r > 48 then print("init", r, a, b, c, d, e, f, g) end
end

-- (4) empty matches and anchors
for r = 1, 40 do
  local a, b = find("abc", "x*")
  local c, d = find("abc", "^")
  local e, f = find("", "")
  local g, h = find("", ".-")
  local i, j = find("abc", "$")
  if r == 40 then print("empty", a, b, c, d, e, f, g, h, i, j) end
end

-- (5) captures and position captures are not recorded: they keep the call, results include the captures
local cap = 0
for r = 1, 100 do
  local a, b, c = find("key=value", "(%w+)=(%w+)")
  local p, q, pos = find("abc", "b()")
  if c == "value" and pos == 3 then cap = cap + 1 end
end
print("cap", cap)

-- (6) the pattern is a variable: capture-free in most iterations and with a capture in some, a malformed one in the last
local pats = { "%a+", "%d+", "(%a+)", "[%a%d]+", "%s*", "(%d)(%d)", "%f[%a]%a+", "%bxy" }
local acc = {}
for r = 1, 160 do
  local pt = pats[r % #pats + 1]
  local a, b, c, d = find("  abc 123 xay ", pt, r % 5)
  acc[#acc + 1] = tostring(a) .. ":" .. tostring(b) .. ":" .. tostring(c) .. ":" .. tostring(d)
end
print("var", #acc, acc[1], acc[2], acc[3], acc[158], acc[159], acc[160])
print("bad", pcall(find, "abc", "[a"))
print("bad2", pcall(find, "abc", "%"))
local errs = 0
for r = 1, 60 do
  local pt = (r == 60) and "[a" or "%a"
  local ok = pcall(find, "abc", pt)
  if not ok then errs = errs + 1 end
end
print("errs", errs)

-- (7) long subject and long pattern strings, a match at the very end, no match at all
local long = string.rep("abcdefghij", 12) .. "XYZ"
local lp = string.rep("[a-j]", 30) .. "X"
local cnt = 0
for r = 1, 100 do
  local a, b = find(long, "%u+$")
  local c, d = find(long, lp)
  local e = find(long, "%d")
  if a and c and not e then cnt = cnt + b - a + d - c end
end
print("long", cnt)

-- (8) a tail call and a call whose results go straight into a table
local function tail(str, pat, init) return find(str, pat, init) end
local t = {}
for r = 1, 100 do
  t[#t + 1] = select("#", tail("abc def", "%a+", r % 7))
  local x = { find("abc def", "%s%a", 1) }
  t[#t + 1] = #x
end
print("tail", #t, t[1], t[2], t[199], t[200])
local function mr(str) return find(str, "%d+") end
local m1, m2 = mr("ab12cd")
print("mr", m1, m2, mr("none"))

-- (9) a pattern whose result depends on a changing subject length (above 40 bytes: a long string), a changing start
local acc2 = 0
for r = 1, 300 do
  local str = string.rep("ab", r % 40) .. "c"
  local a, b = find(str, "b+c")
  if a then acc2 = acc2 + a * 3 + b end
end
print("len", acc2)

-- (10) a pattern with a capture after a long run of capture-free ones at the same call site (the helper must refuse it: the trace has no capture results)
local late = {}
for i = 1, 200 do late[i] = "%a+" end
late[190] = "(%a+)"
late[195] = "()%a"
local seen, ends = 0, 0
for i = 1, 200 do
  local a, b, c = find("abc def", late[i])
  if c then seen = seen + 1 end
  ends = ends + b
end
print("late", seen, ends)
