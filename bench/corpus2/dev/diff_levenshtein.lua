-- Levenshtein distance, LCS-based line diff with unified-diff output, and fuzzy matching of
-- words against a dictionary. small: N=2
local N = tonumber(arg and arg[1]) or 80

local seed = 2024
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

local syll = { "ka", "to", "mi", "re", "su", "no", "la", "vi", "pe", "do", "an", "el" }
local function makeWord()
  local parts = {}
  for i = 1, 2 + rnd(3) do parts[i] = syll[rnd(#syll)] end
  return table.concat(parts)
end

-- Levenshtein ---------------------------------------------------------------------
local function levenshtein(a, b)
  local la, lb = #a, #b
  if la == 0 then return lb end
  if lb == 0 then return la end
  local prev, cur = {}, {}
  for j = 0, lb do prev[j] = j end
  for i = 1, la do
    cur[0] = i
    local ca = a:byte(i)
    for j = 1, lb do
      local cost = (ca == b:byte(j)) and 0 or 1
      local best = prev[j] + 1
      if cur[j - 1] + 1 < best then best = cur[j - 1] + 1 end
      if prev[j - 1] + cost < best then best = prev[j - 1] + cost end
      cur[j] = best
    end
    prev, cur = cur, prev
  end
  return prev[lb]
end

-- Damerau variant with transpositions, full matrix
local function damerau(a, b)
  local d = {}
  for i = 0, #a do d[i] = { [0] = i } end
  for j = 0, #b do d[0][j] = j end
  for i = 1, #a do
    for j = 1, #b do
      local cost = a:sub(i, i) == b:sub(j, j) and 0 or 1
      local v = math.min(d[i - 1][j] + 1, d[i][j - 1] + 1, d[i - 1][j - 1] + cost)
      if i > 1 and j > 1 and a:sub(i, i) == b:sub(j - 1, j - 1) and a:sub(i - 1, i - 1) == b:sub(j, j) then
        v = math.min(v, d[i - 2][j - 2] + 1)
      end
      d[i][j] = v
    end
  end
  return d[#a][#b]
end

-- LCS diff ----------------------------------------------------------------------------
local function lcsTable(a, b)
  local t = {}
  for i = 0, #a do
    t[i] = {}
    for j = 0, #b do t[i][j] = 0 end
  end
  for i = #a, 1, -1 do
    for j = #b, 1, -1 do
      if a[i] == b[j] then
        t[i - 1][j - 1] = t[i][j] + 1
      else
        t[i - 1][j - 1] = math.max(t[i][j - 1], t[i - 1][j])
      end
    end
  end
  return t
end

-- t is indexed shifted: t[i-1][j-1] holds LCS of a[i..], b[j..]
local function diff(a, b)
  local t = lcsTable(a, b)
  local ops = {}
  local i, j = 1, 1
  while i <= #a and j <= #b do
    if a[i] == b[j] then
      ops[#ops + 1] = { " ", a[i] }; i = i + 1; j = j + 1
    elseif t[i][j - 1] >= t[i - 1][j] then
      ops[#ops + 1] = { "-", a[i] }; i = i + 1
    else
      ops[#ops + 1] = { "+", b[j] }; j = j + 1
    end
  end
  while i <= #a do ops[#ops + 1] = { "-", a[i] }; i = i + 1 end
  while j <= #b do ops[#ops + 1] = { "+", b[j] }; j = j + 1 end
  return ops
end

local function unified(ops, context)
  local out = {}
  local n = #ops
  local i = 1
  local aLine, bLine = 1, 1
  while i <= n do
    if ops[i][1] == " " then
      i = i + 1
      aLine = aLine + 1; bLine = bLine + 1
    else
      local s = math.max(1, i - context)
      local e = i
      while e <= n do
        local nextChange = e
        while nextChange <= n and ops[nextChange][1] == " " do nextChange = nextChange + 1 end
        if nextChange > n or nextChange - e > 2 * context then break end
        e = nextChange + 1
      end
      e = math.min(n, e + context)
      local da, db = 0, 0
      local body = {}
      for k = s, e do
        local op = ops[k]
        body[#body + 1] = op[1] .. op[2]
        if op[1] ~= "+" then da = da + 1 end
        if op[1] ~= "-" then db = db + 1 end
      end
      local back = i - s
      out[#out + 1] = string.format("@@ -%d,%d +%d,%d @@", aLine - back, da, bLine - back, db)
      for _, l in ipairs(body) do out[#out + 1] = l end
      for k = i, e do
        if ops[k][1] ~= "+" then aLine = aLine + 1 end
        if ops[k][1] ~= "-" then bLine = bLine + 1 end
      end
      i = e + 1
    end
  end
  return out
end

-- Generate document versions --------------------------------------------------------
local function makeDoc(lines)
  local doc = {}
  for i = 1, lines do
    local w = {}
    for k = 1, 3 + rnd(3) do w[k] = makeWord() end
    doc[i] = table.concat(w, " ")
  end
  return doc
end

local function mutate(doc, edits)
  local out = {}
  for i, l in ipairs(doc) do out[i] = l end
  for _ = 1, edits do
    local r = rnd(3)
    local pos = rnd(#out)
    if r == 1 then table.remove(out, pos)
    elseif r == 2 then table.insert(out, pos, makeWord() .. " " .. makeWord())
    else out[pos] = out[pos] .. " " .. makeWord() end
  end
  return out
end

local v1 = makeDoc(N)
local v2 = mutate(v1, math.max(2, math.floor(N / 4)))
local ops = diff(v1, v2)
local counts = { [" "] = 0, ["-"] = 0, ["+"] = 0 }
for _, op in ipairs(ops) do counts[op[1]] = counts[op[1]] + 1 end
print(string.format("v1=%d lines v2=%d lines: same=%d removed=%d added=%d", #v1, #v2,
  counts[" "], counts["-"], counts["+"]))
local u = unified(ops, 2)
print("unified lines: " .. #u)
for i = 1, math.min(12, #u) do print(u[i]) end

-- A few full diffs of successive versions
local cur, total = v2, 0
for round = 1, 3 do
  local nxt = mutate(cur, 3 + round)
  local o = diff(cur, nxt)
  for _, op in ipairs(o) do if op[1] ~= " " then total = total + 1 end end
  cur = nxt
end
print("changes across rounds: " .. total)

-- Fuzzy matching ---------------------------------------------------------------------------
local dict = {}
for i = 1, 60 + N * 3 do dict[i] = makeWord() end
local function typo(w)
  local r = rnd(4)
  local p = rnd(#w)
  if r == 1 then return w:sub(1, p - 1) .. w:sub(p + 1)
  elseif r == 2 then return w:sub(1, p) .. syll[rnd(#syll)]:sub(1, 1) .. w:sub(p + 1)
  elseif r == 3 and p < #w then return w:sub(1, p - 1) .. w:sub(p + 1, p + 1) .. w:sub(p, p) .. w:sub(p + 2)
  else return w:sub(1, p - 1) .. "x" .. w:sub(p + 1) end
end

local correct, tried, distSum = 0, 0, 0
for q = 1, N * 4 do
  local target = dict[rnd(#dict)]
  local query = typo(target)
  local best, bestD = nil, 1e9
  for _, word in ipairs(dict) do
    local d = levenshtein(query, word)
    if d < bestD or (d == bestD and word < best) then best, bestD = word, d end
  end
  tried = tried + 1
  distSum = distSum + bestD + damerau(query, best)
  if best == target then correct = correct + 1 end
  if q <= 4 then print(string.format("%-12s -> %-12s (d=%d, want %s)", query, best, bestD, target)) end
end
print(string.format("fuzzy: %d/%d correct, distSum=%d", correct, tried, distSum))
print("kitten/sitting " .. levenshtein("kitten", "sitting") .. " " .. damerau("ca", "ac"))
