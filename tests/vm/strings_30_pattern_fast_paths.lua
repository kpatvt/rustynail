-- BUNDLE-B S1/S3: one-item pattern fast paths and the general matcher must agree with the reference
-- (malformed patterns at every position, anchors, captures, gmatch/gsub shapes).
local function strip(e) return (tostring(e):gsub("^[^:]*:%d+: ", "")) end
local function show(...)
  local t = table.pack(...)
  for i = 1, t.n do t[i] = tostring(t[i]) end
  return table.concat(t, "|")
end
local function try(f, ...)
  local r = table.pack(pcall(f, ...))
  if r[1] then return show(table.unpack(r, 2, r.n)) end
  return "ERR " .. strip(r[2])
end

local subjects = { "", "a", "hello, world 42\nsecond line $x", "  lead and trail  ", "aaa", "a.b-c*d+e?f", "x\0y\0z", "ABC123_def" }
local pats = {
  "%a+", "%a*", "%a-", "%a?", "%a", "%A+", "%d+", "%D+", "%s+", "%S+", "%w+", "%W+", "%x+", "%l+", "%u+", "%p+", "%c+",
  "%g+", ".", ".+", ".*", ".-", ".?", "a", "a+", "a*", "a-", "a?", "%$", "%.", "%-", "%%", "%(", "[^\n]+", "[%a_]+",
  "[%d%.]+", "[^%s]+", "[a-c]+", "[^a-c]*", "[WDQMXYFG]", "[]]", "[^]]+", "[a-]", "[%]]", "[%a-z]+", "[z-a]", "[%z]",
  "(%a+)", "(%d*)", "(.)", "(.-)", "(a?)", "()", "()%a+", "(%a)(%a)", "(%a+)(%d+)", "%a+()", "((%a)+)",
  "^%a+", "^", "$", "^$", "%a+$", "^%a+$", "^(%a+)$", "^.-$", ".-$", "a$", "^a", "^^", "a^", "%f[%w]%w+", "%f[%W]",
  "%b()", "%b", "%bx", "(%a)%1", "(a)(b)%2", "%1", "%0",
  -- malformed, lazily detected
  "[a", "[", "[^", "[]", "%", "a%", "%a+%", "(", "(a", ")", "a)", "(()", "%f", "%fa", "%f[", "%b(", "(a)%2", "%9",
  "a[", "a[b", "[a]+[", "x*[", "(%a+", "%a+)", "%a+(", "[%", "[a-", "[a-%", "*", "+", "-", "?", "a**", "a*+", "+a",
  "(*)", "[%a-%d]", "()a()b()",
}
local function many_caps(n)
  return string.rep("(a)", n)
end
pats[#pats + 1] = many_caps(32)
pats[#pats + 1] = many_caps(33)
pats[#pats + 1] = "(" .. string.rep("(", 31) .. "a" .. string.rep(")", 32)

for _, s in ipairs(subjects) do
  local sh = #s > 14 and (s:sub(1, 11) .. "...") or s
  sh = sh:gsub("[%c]", "?")
  print("subject", sh)
  for _, p in ipairs(pats) do
    print(p, "find", try(string.find, s, p), "match", try(string.match, s, p))
    print(p, "find5", try(string.find, s, p, 5), "find-3", try(string.find, s, p, -3), "match0", try(string.match, s, p, 0))
    print(p, "gsub", try(string.gsub, s, p, "<%0>"), "gsub1", try(string.gsub, s, p, "[%1]"), try(string.gsub, s, p, "x", 2))
    print(p, "gsubf", try(string.gsub, s, p, function(...) return "{" .. select("#", ...) .. "}" end))
    print(p, "gsubt", try(string.gsub, s, p, { a = "A", [1] = "one", hello = "HI" }))
    print(p, "gmatch", try(function()
      local out = {}
      for a, b in string.gmatch(s, p) do out[#out + 1] = tostring(a) .. "/" .. tostring(b); if #out > 40 then break end end
      return table.concat(out, ",")
    end))
    print(p, "gmatch-init", try(function()
      local out = {}
      for a in string.gmatch(s, p, 3) do out[#out + 1] = tostring(a); if #out > 40 then break end end
      return table.concat(out, ",")
    end))
  end
end

-- pattern too complex (matchdepth) and its boundary
print(try(string.match, string.rep("a", 300), string.rep("a?", 300) .. "b"))
print(try(string.find, string.rep("a", 190), string.rep("a", 190) .. "b"))
print(try(string.match, string.rep("a", 3000), "^.-b"))
print(#string.match(string.rep("a", 30000), "a*"), #string.match(string.rep("ab", 5000), "[ab]+"))
print(#string.match(string.rep("x", 100) .. "yy", "[^y]+"), #string.match(string.rep("ab,", 100), "[%a,]+"))
print(select(2, string.gsub(string.rep("word ", 200), "%S+", "")))
