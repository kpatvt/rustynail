-- docs/JIT_TRACE.md section 84.2 (`mrset`): constructors filled by an open result list. `{ byte(s, i, e) }` (msgpack's `unpack_uint`) with a
-- count that changes between calls (the count guard exits), a fixed item in front of the open list, a Lua callee returning several values into a
-- constructor, an empty open list, ranges that leave fewer results, the tables read back through `#`, `ipairs`, `next` and `table.concat`.
-- Outputs against the interpreter.
local byte, concat = string.byte, table.concat
local s = ""
for i = 1, 300 do s = s .. string.char((i * 37 + 11) % 256) end

local function uint(i, n)
  local e = i + n - 1
  local b = { byte(s, i, e) }
  local r = 0
  for k = 1, n do r = r * 256 + b[k] end
  return r, #b
end

local function pre(i, n)
  local t = { n, byte(s, i, i + n - 1) }
  local c = 0
  for _, v in ipairs(t) do c = c + v end
  return c + #t
end

local function three(x) return x, x + 1, x * 2 end
local function multi(x)
  local t = { three(x) }
  return t[1] + t[2] + t[3] + #t
end

local function none() end
local function empty()
  local t = { none() }
  return #t + (next(t) == nil and 1 or 0)
end

local acc, cnt = 0, 0
for i = 1, 12000 do
  local n = (i % 7 < 5) and 4 or ((i % 7 == 5) and 2 or 8)
  local p = i % 280 + 1
  local r, len = uint(p, n)
  acc = (acc + r) % 1000000007
  cnt = cnt + len
  acc = (acc + pre(p, (i % 3) + 1)) % 1000000007
  acc = (acc + multi(i)) % 1000000007
  cnt = cnt + empty()
end
print(acc, cnt)

-- ranges at the end of the string: fewer results than asked, and none
local tail = 0
for i = 1, 3000 do
  local p = 295 + i % 8
  local t = { byte(s, p, p + 3) }
  tail = tail + #t
  for k = 1, #t do tail = tail + t[k] end
end
print(tail)

-- the tables escape and are read back later (array part sized by the constructor)
local keep = {}
for i = 1, 800 do
  keep[#keep + 1] = { byte(s, i % 290 + 1, i % 290 + 1 + i % 4) }
end
local sum, parts = 0, {}
for i = 1, #keep, 97 do
  local t = keep[i]
  for k, v in pairs(t) do sum = sum + k * v end
  parts[#parts + 1] = concat(t, ",")
end
print(sum, #parts, parts[1], parts[#parts])
collectgarbage()
print(collectgarbage("count") > 0)
