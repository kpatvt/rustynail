-- String-heavy text munging: word frequency over generated prose, wrapping/justification,
-- capitalisation and naive stemming with gsub. small: N=3
local N = tonumber(arg and arg[1]) or 4500

local seed = 31337
local function pick(list)
  seed = (seed * 16807) % 2147483647
  return list[seed % #list + 1]
end

local nouns = { "garden", "river", "mountain", "engine", "letter", "window", "machines", "stories",
  "teacher", "village", "planets", "bridges", "captain", "doctors", "forest", "paintings" }
local verbs = { "walking", "carried", "watches", "builds", "jumped", "following", "opens", "painted",
  "wandering", "crossed", "discovers", "planning" }
local adjs = { "quiet", "ancient", "bright", "lonely", "curious", "heavy", "gentle", "restless", "golden" }
local dets = { "the", "a", "this", "every", "that", "one" }
local conj = { "and", "but", "while", "because", "although" }

local function clause()
  return table.concat({ pick(dets), pick(adjs), pick(nouns), pick(verbs), "the", pick(adjs), pick(nouns) }, " ")
end

local function sentence()
  local s = clause()
  if #s % 3 == 0 then s = s .. " " .. pick(conj) .. " " .. clause() end
  return s:sub(1, 1):upper() .. s:sub(2) .. (#s % 5 == 0 and "!" or ".")
end

local function paragraph(n)
  local parts = {}
  for i = 1, n do parts[i] = sentence() end
  return table.concat(parts, " ")
end

local paragraphs = {}
for i = 1, N do paragraphs[i] = paragraph(3 + i % 4) end
local text = table.concat(paragraphs, "\n\n")

-- Word frequency ------------------------------------------------------------------
local function wordFreq(s)
  local freq = {}
  for w in s:lower():gmatch("%a+") do
    freq[w] = (freq[w] or 0) + 1
  end
  return freq
end

local function sortedPairs(freq)
  local list = {}
  for w, c in pairs(freq) do list[#list + 1] = { word = w, count = c } end
  table.sort(list, function(a, b)
    if a.count ~= b.count then return a.count > b.count end
    return a.word < b.word
  end)
  return list
end

-- Stemming ------------------------------------------------------------------------
local suffixRules = {
  { "ies$", "y" }, { "ing$", "" }, { "ed$", "" }, { "ers$", "er" }, { "s$", "" },
}
local function stem(w)
  if #w <= 4 then return w end
  for _, rule in ipairs(suffixRules) do
    local new, n = w:gsub(rule[1], rule[2])
    if n > 0 and #new >= 3 then return new end
  end
  return w
end

-- Wrapping & justification -------------------------------------------------------
local function wrap(s, width)
  local lines = {}
  for para in (s .. "\n\n"):gmatch("(.-)\n\n") do
    local line = ""
    for word in para:gmatch("%S+") do
      if #line + #word + 1 > width and #line > 0 then
        lines[#lines + 1] = line
        line = word
      elseif #line == 0 then
        line = word
      else
        line = line .. " " .. word
      end
    end
    if #line > 0 then lines[#lines + 1] = line end
    lines[#lines + 1] = ""
  end
  return lines
end

local function justify(line, width)
  local words = {}
  for w in line:gmatch("%S+") do words[#words + 1] = w end
  if #words < 2 then return line end
  local total = 0
  for _, w in ipairs(words) do total = total + #w end
  local gaps = #words - 1
  local spaces = width - total
  local base = math.floor(spaces / gaps)
  local extra = spaces - base * gaps
  local out = {}
  for i, w in ipairs(words) do
    out[#out + 1] = w
    if i <= gaps then
      out[#out + 1] = string.rep(" ", base + (i <= extra and 1 or 0))
    end
  end
  return table.concat(out)
end

local function titleCase(s)
  return (s:gsub("(%a)([%w']*)", function(first, rest) return first:upper() .. rest:lower() end))
end

local function squeeze(s)
  s = s:gsub("%s+", " ")
  s = s:gsub("^%s+", ""):gsub("%s+$", "")
  return s
end

-- Run ----------------------------------------------------------------------------------
local freq = wordFreq(text)
local sorted = sortedPairs(freq)
print(string.format("chars=%d paragraphs=%d distinct=%d", #text, #paragraphs, #sorted))
for i = 1, 8 do
  print(string.format("%2d. %-10s %5d", i, sorted[i].word, sorted[i].count))
end

local stems = {}
for w, c in pairs(freq) do
  local st = stem(w)
  stems[st] = (stems[st] or 0) + c
end
local sstems = sortedPairs(stems)
print("distinct stems=" .. #sstems)
for i = 1, 5 do print(string.format("stem %-10s %5d", sstems[i].word, sstems[i].count)) end

local width = 60
local wrapped = wrap(text, width)
local justified, maxlen = {}, 0
for i, l in ipairs(wrapped) do
  local j = justify(l, width)
  if #l > 0 and #l < width and l:sub(-1) ~= "." and l:sub(-1) ~= "!" then j = justify(l, width) end
  justified[i] = j
  if #j > maxlen then maxlen = #j end
end
print(string.format("wrapped lines=%d maxlen=%d", #wrapped, maxlen))
for i = 1, 4 do print("|" .. justified[i] .. "|") end

local sum = 0
for i, l in ipairs(justified) do
  for c in l:gmatch("%s") do sum = sum + i % 7 end
end
print("space checksum " .. sum)

local titled = titleCase(paragraphs[1])
print(titled:sub(1, 70))
print((squeeze("   lots   of    odd \t spacing   here  ")))

local caps, bytes = 0, 0
for i = 1, #text do
  local b = text:byte(i)
  bytes = (bytes + b * (i % 13 + 1)) % 1000003
  if b >= 65 and b <= 90 then caps = caps + 1 end
end
print(string.format("caps=%d bytesum=%d upper=%d", caps, bytes, #text:upper():gsub("[^A-Z]", "")))
