-- S6 `gmplus` (docs/JIT_TRACE.md section 64): `string.gmatch` with a pattern that is exactly `%x+` or `[^c]+` / `[^%c]+`
-- (the rn-std direct scan) against the reference: every class letter, the complement forms, init positions, empty and long subjects,
-- bytes outside ASCII, iterators driven by hand after their end, interleaved iterators, coroutines, string metatables, collections.

local function show(s)
  return (s:gsub("[%c\128-\255]", function(c) return string.format("\\%d", c:byte()) end))
end

local function all(s, p, init)
  local out = {}
  local it = init and s:gmatch(p, init) or s:gmatch(p)
  for w in it do out[#out + 1] = show(w) end
  return #out .. ":" .. table.concat(out, "|")
end

local subjects = {
  "", " ", "a", "go north", "  take the   lamp  ", "x9_y-z.w,v;u", "Hello, World! 123 abc_def",
  "line1\nline2\n\nline4\n", "a.b..c...d", "]]a]b^c^^-d-", "tab\tsep\0nul\255hi\127del",
  string.rep("word ", 12) .. "end", string.rep("ab", 30) .. "." .. string.rep("9", 45),
}
local letters = "acdglpsuwxz"
for _, s in ipairs(subjects) do
  local row = {}
  for i = 1, #letters do
    local l = letters:sub(i, i)
    row[#row + 1] = all(s, "%" .. l .. "+")
    row[#row + 1] = all(s, "%" .. l:upper() .. "+")
    row[#row + 1] = all(s, "[^%" .. l .. "]+")
    row[#row + 1] = all(s, "[^%" .. l:upper() .. "]+")
  end
  for _, c in ipairs({ "\n", ",", ".", "]", "^", "-", " ", "a", "\0" }) do
    row[#row + 1] = all(s, "[^" .. c .. "]+")
    row[#row + 1] = all(s, "[^%" .. c .. "]+")
  end
  row[#row + 1] = all(s, "[^%%]+")
  print(show(s), table.concat(row, " / "))
end

-- init positions (positive, negative, beyond the end)
local s = "alpha beta  gamma"
for _, init in ipairs({ 1, 2, 6, 7, 17, 18, 19, 100, 0, -1, -5, -17, -100 }) do
  print("init", init, all(s, "%a+", init), all(s, "[^ ]+", init))
end

-- the iterator called by hand past its end keeps answering nothing
local it = ("one two"):gmatch("%a+")
print(it(), it(), it(), it(), select("#", it()))
local it2 = ("..."):gmatch("[^.]+")
print(select("#", it2()), it2())

-- interleaved iterators over the same subject and pattern
local a, b = ("p q r s"):gmatch("%a+"), ("p q r s"):gmatch("%a+")
print(a(), a(), b(), a(), b(), b(), b(), a(), b())

-- the general matcher for neighbouring shapes (not the direct scan)
for _, p in ipairs({ "%a", "%a*", "%a-", "(%a+)", "%a+$", "[^ab]+", "[a]+", "[^a-c]+", "a+", "%a++", "(%a+)%s*" }) do
  print(p, all("abc de-f ab++", p))
end
local ok, msg = pcall(function() for w in ("abc"):gmatch("[^%]+") do end end)
print(ok, (msg:gsub("^.-:%d+: ", "")))

-- string metatable with __index / __tostring changes nothing for gmatch
local mt = getmetatable("")
local old = mt.__tostring
mt.__tostring = function() return "X" end
print(all("meta table test", "%a+"))
mt.__tostring = old

-- inside coroutines, yielding between matches
local co = coroutine.wrap(function()
  for w in ("co ro ut ine"):gmatch("%a+") do coroutine.yield(w) end
  return "done"
end)
print(co(), co(), co(), co(), co())

-- many iterations with collections in between (the captured strings stay valid)
local words, n = {}, 0
for i = 1, 300 do
  local line = "w" .. i .. " x" .. (i * 7) .. " y_" .. (i % 13) .. " " .. string.rep("z", i % 50)
  for w in line:gmatch("%w+") do n = n + #w; words[#words + 1] = w end
  if i % 50 == 0 then collectgarbage() end
end
print(n, #words, words[1], words[#words], words[600])
local cnt = 0
for w in string.rep("a,bb,,ccc,", 200):gmatch("[^,]+") do cnt = cnt + #w end
print(cnt)
