-- gmatch iterators in generic-for position (hot loops: recorded / helper paths must agree with the C iterator)
local out = {}
local function p(...)
  local t = table.pack(...)
  for i = 1, t.n do t[i] = tostring(t[i]) end
  out[#out + 1] = table.concat(t, "|")
end

-- one capture, whole match, no captures
local text = "the quick brown fox jumps over the lazy dog"
for round = 1, 300 do
  local n, len, caps = 0, 0, 0
  for w in text:gmatch("%a+") do n = n + 1; len = len + #w end
  for w in text:gmatch("(%a)%a*") do caps = caps + #w end
  for w, extra in text:gmatch("%a+") do assert(extra == nil); n = n + 1 end
  for w in string.gmatch(text, "o") do n = n + 1 end
  if round % 100 == 0 then p("words", round, n, len, caps) end
end

-- several captures and position captures (not helper shapes; must still work inside hot loops)
for round = 1, 200 do
  local s = ""
  for k, v in ("a=1, b=22, c=333"):gmatch("(%w+)=(%w+)") do s = s .. k .. v end
  for pos, w in ("ab cd ef"):gmatch("()(%a+)") do s = s .. pos .. w end
  for pos in ("ab cd ef"):gmatch("()%a+") do s = s .. pos end
  if round % 100 == 0 then p("multi", round, s) end
end

-- empty matches and the lastmatch rule
for round = 1, 200 do
  local c = {}
  for w in ("abc"):gmatch("") do c[#c + 1] = "[" .. w .. "]" end
  for w in ("abc"):gmatch("x*") do c[#c + 1] = "<" .. w .. ">" end
  for w in ("a,b,,c"):gmatch("([^,]*)") do c[#c + 1] = "(" .. w .. ")" end
  for w in (""):gmatch("a*") do c[#c + 1] = "{" .. w .. "}" end
  if round % 100 == 0 then p("empty", round, table.concat(c)) end
end

-- anchors: '^' in gmatch is literal-start-anchored per the reference (matches at the init only)
for round = 1, 200 do
  local c = {}
  for w in ("hello hello"):gmatch("^hello") do c[#c + 1] = w end
  for w in ("^a^a"):gmatch("%^a") do c[#c + 1] = w end
  for w in ("hello hello"):gmatch("hello$") do c[#c + 1] = w end
  if round % 100 == 0 then p("anchor", round, #c, table.concat(c, ",")) end
end

-- init argument
for round = 1, 200 do
  local c = {}
  local s = "one two three four"
  for w in s:gmatch("%a+", 5) do c[#c + 1] = w end
  for w in s:gmatch("%a+", -4) do c[#c + 1] = w end
  for w in s:gmatch("%a+", 100) do c[#c + 1] = w end
  for w in s:gmatch("%a+", -100) do c[#c + 1] = w end
  for w in s:gmatch("%a*", #s + 1) do c[#c + 1] = "<" .. w .. ">" end
  if round % 100 == 0 then p("init", round, table.concat(c, ",")) end
end

-- nested gmatch, long strings, long captures
do
  local lines = {}
  for i = 1, 40 do lines[#lines + 1] = ("w%d x%d y%d"):format(i, i * 2, i * 3) end
  local doc = table.concat(lines, "\n")
  local long = ("z"):rep(200) .. " " .. ("q"):rep(50)
  for round = 1, 150 do
    local total = 0
    for line in doc:gmatch("[^\n]+") do
      for w in line:gmatch("%S+") do total = total + #w end
    end
    local ll = 0
    for w in long:gmatch("%S+") do ll = ll + #w end
    for w in long:gmatch(".") do ll = ll + 1 end
    if round % 75 == 0 then p("nested", round, total, ll) end
  end
end

-- short and long captures mixed in one loop (the recorded result length class differs from the next iteration's)
do
  local mix1 = "ab " .. ("x"):rep(50) .. " cd " .. ("y"):rep(45) .. " e"
  local mix2 = ("p"):rep(60) .. " q " .. ("r"):rep(41) .. " s tt"
  for round = 1, 200 do
    local t1, t2, t3 = 0, 0, 0
    for w in mix1:gmatch("%S+") do t1 = t1 * 3 + #w end
    for w in mix2:gmatch("%S+") do t2 = t2 * 3 + #w end
    for w in (mix1 .. " " .. mix2):gmatch("(%S+)") do t3 = t3 * 7 + #w end
    if round % 100 == 0 then p("mix", round, t1, t2, t3) end
  end
end

-- iterator reuse, early break, exhaustion
for round = 1, 200 do
  local it = ("a b c d"):gmatch("%a")
  local got = {}
  for w in it do got[#got + 1] = w; if w == "b" then break end end
  for w in it do got[#got + 1] = w end
  got[#got + 1] = tostring((it()))
  got[#got + 1] = tostring((it()))
  if round % 100 == 0 then p("reuse", round, table.concat(got, ",")) end
end

-- the iterator called by hand and mixed with a for loop
for round = 1, 200 do
  local it = ("1 2 3 4 5 6"):gmatch("%d")
  local a, b = it(), it()
  local s = a .. b
  for d in it do s = s .. d end
  if round % 100 == 0 then p("mixed", round, s) end
end

-- the subject variable changes while iterating (the iterator holds the original string)
for round = 1, 200 do
  local s = "x1 x2 x3"
  local c = {}
  for w in s:gmatch("x%d") do s = s .. "!"; c[#c + 1] = w end
  if round % 100 == 0 then p("subject", round, s, table.concat(c, ",")) end
end

-- the pattern changes in the middle of a loop (several gmatch closures alive)
for round = 1, 200 do
  local c = {}
  local pats = { "%a+", "%d+", "%s" }
  for i = 1, 3 do
    for w in ("ab 12 cd 34"):gmatch(pats[i]) do c[#c + 1] = w end
  end
  if round % 100 == 0 then p("pats", round, table.concat(c, ",")) end
end

-- errors inside the loop (the C function raises them with the same messages)
for round = 1, 120 do
  local ok1, e1 = pcall(function() for w in ("abc"):gmatch("[a") do end end)
  local ok2, e2 = pcall(function() for w in ("abc"):gmatch("%") do end end)
  local ok3, e3 = pcall(function() for w in ("abc"):gmatch("(a") do end end)
  local ok4, e4 = pcall(function() for w in ("abc"):gmatch("%1") do end end)
  local ok5, e5 = pcall(function() for w in ("abc"):gmatch("(()") do end end)
  -- an error after some matches: the pattern is fine on the first matches only
  local ok6, e6 = pcall(function()
    local n = 0
    for w in ("aaaa"):gmatch(".-%b") do n = n + 1 end
    return n
  end)
  if round % 60 == 0 then p("errors", round, ok1, e1, ok2, e2, ok3, e3, ok4, e4, ok5, e5, ok6, e6) end
end

-- inside coroutines, with yields in the loop body
for round = 1, 100 do
  local co = coroutine.wrap(function(s)
    for w in s:gmatch("%a+") do coroutine.yield(w) end
    return "done"
  end)
  local r = { co("alpha beta gamma delta") }
  for _ = 1, 4 do r[#r + 1] = co() end
  if round % 50 == 0 then p("coro", round, table.concat(r, ",")) end
end

-- a loop whose body allocates a lot (collector steps inside the loop), keeping the results
for round = 1, 20 do
  local keep = {}
  local s = ("item%d "):rep(400)
  local i = 0
  for w in s:gmatch("%S+") do
    i = i + 1
    keep[#keep + 1] = w .. tostring(i) .. ("x"):rep(i % 7)
  end
  if round % 10 == 0 then p("alloc", round, #keep, keep[1], keep[#keep]) end
end

-- results are real strings: usable as table keys, comparable, identical for equal contents
for round = 1, 100 do
  local seen, n = {}, 0
  for w in ("a b a c b a"):gmatch("%a") do
    if not seen[w] then seen[w] = 0; n = n + 1 end
    seen[w] = seen[w] + 1
  end
  if round % 50 == 0 then p("keys", round, n, seen.a, seen.b, seen.c) end
end

-- more variables than values
for round = 1, 100 do
  local c = {}
  for a, b, c3 in ("x y z"):gmatch("%a") do c[#c + 1] = tostring(a) .. tostring(b) .. tostring(c3) end
  if round % 50 == 0 then p("vars", round, table.concat(c, ",")) end
end

print(table.concat(out, "\n"))
