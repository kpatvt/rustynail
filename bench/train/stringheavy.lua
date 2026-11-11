-- PGO training (scripts/train.sh): string-library and string-creation heavy work: concatenation, format,
-- find/match/gmatch/gsub with patterns, rep/sub/byte/char/upper/lower/reverse, tostring/tonumber, table.concat,
-- interning of many short strings, a few long ones. Deterministic.
math.randomseed(7)
local fmt, sub, byte, char, find, match, gsub, rep = string.format, string.sub, string.byte, string.char, string.find, string.match, string.gsub, string.rep

local function words(n)
  local t = {}
  for i = 1, n do
    local len = 3 + (i * 7) % 8
    local w = {}
    for j = 1, len do w[j] = char(97 + (i * j + j) % 26) end
    t[i] = table.concat(w)
  end
  return t
end

local ws = words(8000)
local text = table.concat(ws, " ")
print(#text)

-- scanning
local counts = {}
for w in text:gmatch("%a+") do counts[w] = (counts[w] or 0) + 1 end
local distinct = 0
for _ in pairs(counts) do distinct = distinct + 1 end
print(distinct)

-- searching and substitution
local hits = 0
for i = 1, 4000 do
  local w = ws[i]
  if find(text, w, 1, true) then hits = hits + 1 end
  if match(w, "^[a-m]+$") then hits = hits + 1 end
end
print(hits)
local up = gsub(text, "(%a)(%a*)", function(a, b) return a:upper() .. b end)
print(#up, #gsub(up, "%s+", "_"), (gsub(text, "e", "E")):sub(1, 20))
local n = 0
for k, v in text:sub(1, 20000):gmatch("(%a+)%s+(%a+)") do n = n + #k + #v end
print(n)

-- formatting and conversion
local out = {}
for i = 1, 30000 do
  out[#out + 1] = fmt("%5d|%-8s|%08.3f|%x|%q", i, ws[i % #ws + 1], i / 7, i * 31, "q" .. i)
end
print(#table.concat(out, "\n"))
local s = 0
for i = 1, 60000 do
  s = s + tonumber(tostring(i)) + tonumber(fmt("%.3f", i / 3)) + #tostring(i / 7)
end
print(s)

-- byte-level work
local b = 0
for i = 1, #text, 3 do b = b + byte(text, i) end
local chars = {}
for i = 1, 20000 do chars[i] = char(32 + i % 90) end
print(b, #table.concat(chars), #text:reverse(), #text:upper(), #rep("ab", 5000, ","))

-- many short-lived strings through concatenation and interning
local acc = 0
for i = 1, 200000 do
  local k = "id" .. i % 997 .. "_" .. (i % 13)
  acc = acc + #k
  counts[k] = i
end
print(acc)
local parts = {}
for i = 1, 3000 do parts[i] = ws[i] .. "=" .. i end
local joined = table.concat(parts, "&")
local parsed = 0
for k, v in joined:gmatch("([^&=]+)=([^&=]+)") do parsed = parsed + #k + tonumber(v) end
print(#joined, parsed)
table.sort(ws)
print(ws[1], ws[#ws])
