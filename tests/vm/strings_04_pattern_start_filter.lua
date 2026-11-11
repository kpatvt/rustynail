-- Pattern matcher start-position filter (`MatchState::start_filter`/`skip`): patterns whose first
-- item is a literal or a single-character class are only tried where that item matches. Covers
-- literals, classes, sets, '.', optional first items (no filter), anchors, init beyond the end,
-- empty matches, captures/position captures, gsub copying of skipped text, and `s:method()` on
-- strings (SELF fast path), including a replaced string metatable `__index`.
local function show(...)
  local t = table.pack(...)
  for i = 1, t.n do t[i] = tostring(t[i]) end
  return table.concat(t, ",")
end

local subjects = {"", "a", "xyz", "12:item,345:item;item", "aaa bbb  ccc", "$a$b", "^x^", "%d%d", "\0z\0"}
local pats = {"item", "%d+:item", "%d+", "[%a_]+", "[^%s]+", ".", "a", "a*", "a?b", "a-b", "%s",
  "%S+", "$", "$a", "^a", "^x", "%%d", "(%d+):(%a+)", "()item", "i()t", "%z", "%Z+", "[]]", "b+$",
  "%f[%w]%w+", "%bab", "(a)%1", "[a-c]+", "x", "z", "\0"}
for _, s in ipairs(subjects) do
  for _, p in ipairs(pats) do
    for _, init in ipairs({1, 2, -1, 100}) do
      print(show(s:find(p, init)), show(s:match(p, init)))
    end
    local g = {}
    for a, b in s:gmatch(p) do g[#g + 1] = show(a, b) end
    print("gmatch", p, table.concat(g, "|"))
    print("gsub", show(s:gsub(p, "<%0>")), show(s:gsub(p, "#", 1)), show(s:gsub(p, {item = "I"})))
    print("gsubf", show(s:gsub(p, function(...) return select("#", ...) .. "" end)))
  end
end

-- plain find with multi-byte needles, overlaps and needles at the end
local hay = string.rep("ab", 20) .. "abc" .. string.rep("x", 13) .. "abc"
for _, n in ipairs({"abc", "ba", "xa", "xab", "c", "cx", "xxxxxxxxxabc", "q", ""}) do
  print(n, hay:find(n, 1, true), hay:find(n, 10, true), hay:find(n, -3, true))
end

-- long subjects (word-at-a-time scan) with the byte at every offset
for off = 0, 20 do
  local s = string.rep("-", off) .. "k" .. string.rep("-", 20 - off)
  print(off, s:find("k"), s:find("k-"), (s:gsub("k", "K")))
end

-- SELF on strings: methods, missing methods, replaced __index
local s = "hello"
print(s:upper(), s:sub(2, 3), s:len(), pcall(function() return s:nosuch() end))
local mt = getmetatable("")
local old = mt.__index
mt.__index = setmetatable({shout = function(x) return x:upper() .. "!" end}, {__index = old})
print(s:shout(), s:rep(2))
mt.__index = function(str, k) return function() return k .. "@" .. str end end
print(s:anything(), s:upper())
mt.__index = old
print(s:upper())
