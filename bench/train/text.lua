-- PGO training (scripts/pgo.sh): string and table library work unlike bench/programs
-- (CSV parsing, a template expander, word statistics, a serializer, sorting with comparators).
local rows = {}
for i = 1, 4000 do
  rows[#rows + 1] = string.format("%d,%s,%0.2f,%s", i, "name" .. (i * 7919 % 1000), i / 7, i % 2 == 0 and "yes" or "no")
end
local csv = table.concat(rows, "\n")
local parsed, sum = {}, 0
for line in csv:gmatch("[^\n]+") do
  local id, name, val, flag = line:match("^(%d+),([^,]*),([^,]*),(%a+)$")
  parsed[#parsed + 1] = {id = tonumber(id), name = name, val = tonumber(val), flag = flag == "yes"}
  sum = sum + tonumber(val)
end
print(#parsed, string.format("%.2f", sum))

table.sort(parsed, function(a, b) if a.name ~= b.name then return a.name < b.name end return a.id < b.id end)
print(parsed[1].name, parsed[#parsed].name)

local template = "Dear ${name}, your order #${id} of ${count} item(s) totals ${total}. ${extra}"
local out = {}
for i = 1, 3000 do
  local vars = {name = "user" .. i, id = i, count = i % 9, total = string.format("%.2f", i * 1.25), extra = ("!"):rep(i % 4)}
  out[#out + 1] = template:gsub("%${(%w+)}", vars)
end
print(#table.concat(out))

local text = ("the quick brown fox jumps over the lazy dog and keeps running far away "):rep(400)
local freq, words = {}, 0
for w in text:gmatch("%a+") do
  freq[w] = (freq[w] or 0) + 1
  words = words + 1
end
local keys = {}
for k in pairs(freq) do keys[#keys + 1] = k end
table.sort(keys, function(a, b) return freq[a] > freq[b] or (freq[a] == freq[b] and a < b) end)
print(words, keys[1], freq[keys[1]])

local function serialize(v, buf)
  local t = type(v)
  if t == "table" then
    buf[#buf + 1] = "{"
    local ks = {}
    for k in pairs(v) do ks[#ks + 1] = k end
    table.sort(ks, function(a, b) return tostring(a) < tostring(b) end)
    for _, k in ipairs(ks) do
      buf[#buf + 1] = "[" .. string.format("%q", k) .. "]="
      serialize(v[k], buf)
      buf[#buf + 1] = ","
    end
    buf[#buf + 1] = "}"
  elseif t == "string" then
    buf[#buf + 1] = string.format("%q", v)
  else
    buf[#buf + 1] = tostring(v)
  end
end
local big = {}
for i = 1, 300 do big["k" .. i] = {i, tostring(i), {deep = i % 5 == 0, s = ("x"):rep(i % 13)}} end
for _ = 1, 8 do
  local buf = {}
  serialize(big, buf)
  local s = table.concat(buf)
  local f = load("return " .. s)
  local back = f()
  assert(back.k7[2] == "7")
end
print("serialized")

local acc = 0
for i = 1, 30000 do
  local s = tostring(i) .. ":" .. i * 2
  acc = acc + #s + s:byte(1) + (s:find(":", 1, true) or 0)
  local u = s:upper():lower():sub(2, -2):reverse()
  acc = acc + #u
  if s:sub(-1) == "0" then acc = acc + #string.rep(s, 3, "-") end
  local a, b = string.char(65 + i % 26, 97 + i % 26):byte(1, 2)
  acc = acc + a + b
end
print(acc)

local t = {}
for i = 1, 20000 do table.insert(t, i) end
for i = 1, 5000 do table.remove(t, 1 + (i * 31) % #t) end
local moved = table.move(t, 1, 100, 1, {})
print(#t, #moved, select("#", table.unpack(moved)), utf8.len("héllo wörld"), utf8.char(72, 228))
