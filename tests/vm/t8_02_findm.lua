-- docs/JIT_TRACE.md section 44.2 (T8, `libcall` bit 11): `string.match` with no capture or one string capture recorded as the FINDM library op followed by the
-- substring. Same output as the reference in every mode.
local match, sub = string.match, string.sub

-- (1) a lexer shaped loop: the whole match and a capture, anchored at a moving position
local src = "local x = 10 + foo(bar, 'str') -- comment\nreturn x * 2.5e3 .. \"s\" ; y = 0x1F + 3"
local function lex(s)
  local out, pos, n = {}, 1, #s
  while pos <= n do
    local ws = match(s, "^%s+", pos)
    if ws then
      pos = pos + #ws
    else
      local id = match(s, "^[%a_][%w_]*", pos)
      if id then
        out[#out + 1] = id
        pos = pos + #id
      else
        local num = match(s, "^(%d+%.?%d*)", pos)
        if num then
          out[#out + 1] = "#" .. num
          pos = pos + #num
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

-- (2) one capture in the middle, an empty capture, a capture at the end, the whole match when there is none
local lines = { "key=value", "k = v", "# comment", "x=", "=y", "", "name: alice", "  indented", "a=b=c" }
local acc = {}
for r = 1, 180 do
  local l = lines[r % #lines + 1]
  local k = match(l, "^(%w+)=")
  local v = match(l, "=(.*)$")
  local w = match(l, "%a+")
  local e = match(l, "^()")
  acc[#acc + 1] = tostring(k) .. "/" .. tostring(v) .. "/" .. tostring(w) .. "/" .. tostring(e)
end
print("caps", #acc, table.concat(acc, ",", 1, 9), acc[180])

-- (3) several captures and position captures stay calls (all their results come back)
local n = 0
for r = 1, 100 do
  local a, b = match("key=value", "(%w+)=(%w+)")
  local p = match("hello", "()ll")
  local x, y, z = match("2024-05-17", "(%d+)-(%d+)-(%d+)")
  if a == "key" and b == "value" and p == 3 and z == "17" then n = n + 1 end
end
print("multi", n)

-- (4) init: negative, zero, past the end; no match; match at the very end
local s = "hello world, hello lua"
for r = 1, 50 do
  local a = match(s, "l+", r - 25)
  local b = match(s, "%a+$", -3)
  local c = match(s, "x*", #s + 1)
  local d = match(s, "x*", #s + 2)
  local e = match(s, "(%d)", r)
  if r > 48 then print("init", r, a, b, c, d, e) end
end

-- (5) the pattern changes from one capture to two (the helper refuses, the call is made), and the string length class of the result changes
local pats = { "^(%a+)", "(%a+) (%a+)", "%a+", "(%d)", "^(.-),", "^(.*)$" }
local res = {}
for r = 1, 120 do
  local str = string.rep("ab ", r % 20) .. "end, x"
  local a, b = match(str, pats[r % #pats + 1])
  res[#res + 1] = (a and #a or -1) .. ":" .. tostring(b and #b)
end
print("var", #res, table.concat(res, ",", 1, 12), res[120])

-- (6) malformed patterns raise the library's error
print("bad", pcall(match, "abc", "[a"))
print("bad2", pcall(match, "abc", "(a"))
local errs = 0
for r = 1, 60 do
  local ok = pcall(match, "abc", r == 60 and "(a" or "%a")
  if not ok then errs = errs + 1 end
end
print("errs", errs)

-- (7) the result is used as a key and compared (an interned string)
local t = { foo = 1, bar = 2 }
local sum = 0
for r = 1, 100 do
  local k = match(r % 2 == 0 and "foo=1" or "bar=2", "^(%a+)")
  sum = sum + t[k]
end
print("key", sum)

-- (8) a tail call, and results spread into a call
local function tail(str, pat) return match(str, pat) end
local c = 0
for r = 1, 100 do
  if tail("abc def", "%a+") == "abc" then c = c + 1 end
end
print("tail", c, select("#", tail("abc", "x")), select("#", tail("abc", "(a)")))

-- (9) a long subject and a long result
local long = string.rep("x", 30) .. "YYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYY" .. string.rep("z", 10)
local lens = 0
for r = 1, 100 do
  local m = match(long, "Y+")
  local n2 = match(long, "^x*(Y+)z*$")
  lens = lens + #m + #n2
end
print("long", lens)

-- (10) several captures / a position capture after a long run of single-capture patterns at the same call site (the helper must refuse them)
local late = {}
for i = 1, 200 do late[i] = "^(%a+)" end
late[190] = "(%a)(%a+)"
late[195] = "()%a+"
local out2 = {}
for i = 1, 200 do
  local a, b = match("abc def", late[i])
  if i >= 188 then out2[#out2 + 1] = tostring(a) .. "/" .. tostring(b) end
end
print("late", table.concat(out2, ","))
