-- BUNDLE-S3 `patc`: compiled pattern cache. Every pattern is used several times (first sight, compile, cached),
-- with the same string object and with equal strings at other addresses; results and errors must equal the reference.
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

local subjects = {
  "", "x", "key = value; other=thing ; n=42", "<a href=\"x.html\">link</a> <b>bold</b> <i>it</i>",
  "THE (quick) [brown] {fox} jumps", "2024-01-15T10:20:30Z 1999-12-31", "  trim me  \t\n", "aXbXXcXXXd",
  "a,b,,c,,,d", "foo.bar.baz", "((nested (parens)) here)", "x = f(1, g(2, 3)) + h()", "\0\1\2abc\255", "ababababab",
}
local pats = {
  "%s*(%w+)%s*=%s*(%w+)", "(%d+)-(%d+)-(%d+)", "<(%a+)[^>]*>(.-)</%1>", "^%s*(.-)%s*$", "%f[%a]%a+%f[%A]",
  "%b()", "%((.-)%)", "[%[%]{}()]", "(%w+)%.(%w+)", "([^,]*),?", "(.-)X+", "X+", "a.-b", "a.*b", "a.+b", ".-c",
  "(a)(b)", "((a)(b))", "()ab()", "%d%d?%d?", "[A-Z][a-z]*", "^(.)(.*)$", "(.)%1", "(%a)(%a)%2%1", "%s+$", "^%s+",
  "[^%s]+", "%w+@%w+", "x?y?z?", "a*b*c*", "[a-c]+[x-z]*", "%a+%d*%a*", "(%b())", "f%((.-)%)", ".-$", "%.", "%%",
  "[%w_]+", "[^=;]+", "=([^;]*)", "(%w+)=(%w+)", "\0", "[\0-\2]+", "\255", "%z", "[%z]+", "b?a", "ab-a", "^ab", "ab$",
  "$a", "a^b", "^^a", "a?b?c?d?e?", "(a?)(b?)", "()", "(())", "%f[%w]", "%f[%W]", "(%f[%w]%w+)",
  -- malformed: the error appears only when matching reaches the item
  "x[", "x[a", "x%", "a*[", "a*%", "x(", "x)", "x%b", "x%bx", "x%f", "x%fa", "x%f[", "x%1", "x%0", "(x)%2", "(x",
  "a-%", "k[", "e[a-", "[", "[a", "%", "%b", "%bx", "%f", "%f[a", "(", ")", "%1", "(a)(b)%3", "n%", "=%", "e%b", "e%f",
  "(%a+)(", "((a)", "(a))", "a)(b",
}

local function run(p, s)
  local out = {}
  out[#out + 1] = try(string.find, s, p)
  out[#out + 1] = try(string.match, s, p)
  out[#out + 1] = try(string.find, s, p, 3)
  out[#out + 1] = try(string.find, s, p, -4)
  out[#out + 1] = try(string.match, s, p, 2)
  out[#out + 1] = try(string.gsub, s, p, "<%0>")
  out[#out + 1] = try(string.gsub, s, p, "[%1]", 2)
  out[#out + 1] = try(string.gsub, s, p, function(...) return "{" .. select("#", ...) .. "}" end)
  out[#out + 1] = try(string.gsub, s, p, { a = "A", key = "K", ["x"] = false })
  out[#out + 1] = try(function()
    local o = {}
    for a, b in string.gmatch(s, p) do o[#o + 1] = tostring(a) .. "/" .. tostring(b); if #o > 40 then break end end
    return table.concat(o, ",")
  end)
  out[#out + 1] = try(function()
    local o = {}
    for a in string.gmatch(s, p, 4) do o[#o + 1] = tostring(a); if #o > 40 then break end end
    return table.concat(o, ",")
  end)
  return table.concat(out, "\n  ")
end

-- the same pattern object over several rounds (the first call only notes the address, the second compiles)
for pi, p in ipairs(pats) do
  for si, s in ipairs(subjects) do
    local first = run(p, s)
    for round = 1, 3 do
      assert(run(p, s) == first, "unstable result for " .. p)
    end
    print(pi, si, first:sub(1, 400))
  end
end

-- equal strings built at run time (other addresses, garbage collected between rounds: addresses get reused)
for round = 1, 6 do
  for pi = 1, #pats, 7 do
    local p = (pats[pi] .. "#"):sub(1, -2)
    local s = subjects[(pi % #subjects) + 1]
    print("dyn", round, pi, (run(p, s):gsub("\n", ";")):sub(1, 200))
  end
  collectgarbage()
end

-- address reuse: patterns of the same length are created and dropped; a different pattern must never use an old program
local function churn(n)
  local acc = {}
  for i = 1, n do
    local len = 4 + i % 5
    local p = string.rep("%a", len, "") -- "%a%a.."
    p = p:sub(1, len - 1) .. (i % 2 == 0 and "d" or "a")
    acc[#acc + 1] = show(string.find("abcdef12345 xyz", p))
    acc[#acc + 1] = show(string.find("abcdef12345 xyz", "[" .. (i % 3 == 0 and "%d" or "%a") .. "]+" .. string.rep("x", i % 3)))
    if i % 7 == 0 then collectgarbage() end
  end
  return table.concat(acc, ";")
end
local c1 = churn(300)
print(#c1, c1:sub(1, 120))
for _ = 1, 3 do assert(churn(300) == c1) end

-- collectgarbage("count") does not depend on the pattern cache: a loop over patterns keeps the same
-- collectable footprint once everything it allocates is dropped
collectgarbage("collect"); collectgarbage("stop")
local before = collectgarbage("count")
for i = 1, 50 do
  for _, p in ipairs({ "%a+", "(%w+)=(%w+)", "^%s*(.-)%s*$", "[%d%.]+" }) do
    local _ = ("key=val 12.5 abc"):find(p)
  end
end
local after = collectgarbage("count")
-- the loop allocates nothing that survives except captured strings (interned short strings): bounded growth
print("count stable", after - before < 64)
collectgarbage("restart")

-- patterns longer than the cache limit, and the limit boundary, use the general matcher with the same results
for _, n in ipairs({ 100, 511, 512, 513, 600 }) do
  local p = string.rep("a?", n // 2) .. (n % 2 == 1 and "b" or "")
  p = p:sub(1, n)
  local s = string.rep("a", 90) .. "b"
  for _ = 1, 3 do print("long", n, try(string.find, s, p), try(string.match, s, p)) end
end

-- pattern too complex at and around the recursion limit (MAXCCALLS 200)
for _, n in ipairs({ 190, 196, 198, 199, 200, 201, 202, 210, 300 }) do
  for _, unit in ipairs({ "a?", "a-", "(a)", ".-b?", "a*b?" }) do
    local p = string.rep(unit, n)
    local s = string.rep("a", 400)
    for _ = 1, 3 do print("deep", unit, n, try(string.find, s, p), try(string.match, s, p)) end
  end
end
for n = 28, 36 do
  for _ = 1, 3 do
    print("caps", n, try(string.match, string.rep("a", 40), string.rep("(a)", n)), try(string.find, string.rep("a", 40), string.rep("()", n)))
  end
end

-- gsub/gmatch iterators interleaved with other pattern calls (cache entries replaced while an iterator is alive)
local it1 = string.gmatch("one two three four five six", "%a+")
local it2 = string.gmatch("1 22 333 4444", "%d+")
local o = {}
for _ = 1, 6 do
  o[#o + 1] = tostring((it1())) .. ":" .. tostring((it2()))
  for k = 1, 40 do local _ = ("abc" .. k):find("%d+" .. string.rep("x", k % 5)) end
end
print(table.concat(o, " "))
print(("hello world"):gsub("%w+", function(w) return (w:gsub("o", "0")) .. ("x"):rep(2):upper() end))
