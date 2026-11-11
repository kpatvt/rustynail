-- lua-MessagePack (fperrad) style pure-Lua encoder/decoder, Lua 5.1-compatible arithmetic paths
-- (no string.pack): packers dispatch table by type, cursor-based unpacker with an underflow callback.
local floor, char, byte, tconcat = math.floor, string.char, string.byte, table.concat
local m = {}

local packers = setmetatable({}, { __index = function(t, k) error("pack '" .. k .. "' is unimplemented") end })
m.packers = packers

packers['nil'] = function(buffer) buffer[#buffer + 1] = char(0xC0) end
packers['boolean'] = function(buffer, bool)
  if bool then buffer[#buffer + 1] = char(0xC3) else buffer[#buffer + 1] = char(0xC2) end
end

packers['string'] = function(buffer, str)
  local n = #str
  if n <= 0x1F then
    buffer[#buffer + 1] = char(0xA0 + n)
  elseif n <= 0xFF then
    buffer[#buffer + 1] = char(0xD9, n)
  elseif n <= 0xFFFF then
    buffer[#buffer + 1] = char(0xDA, floor(n / 0x100), n % 0x100)
  else
    buffer[#buffer + 1] = char(0xDB, floor(n / 0x1000000), floor(n / 0x10000) % 0x100, floor(n / 0x100) % 0x100, n % 0x100)
  end
  buffer[#buffer + 1] = str
end

packers['map'] = function(buffer, tbl, n)
  if n <= 0x0F then
    buffer[#buffer + 1] = char(0x80 + n)
  elseif n <= 0xFFFF then
    buffer[#buffer + 1] = char(0xDE, floor(n / 0x100), n % 0x100)
  else
    buffer[#buffer + 1] = char(0xDF, floor(n / 0x1000000), floor(n / 0x10000) % 0x100, floor(n / 0x100) % 0x100, n % 0x100)
  end
  -- deterministic key order
  local keys = {}
  for k in pairs(tbl) do keys[#keys + 1] = k end
  table.sort(keys, function(a, b)
    local ta, tb = type(a), type(b)
    if ta ~= tb then return ta < tb end
    return a < b
  end)
  for i = 1, #keys do
    local k = keys[i]
    packers[type(k)](buffer, k)
    packers[type(tbl[k])](buffer, tbl[k])
  end
end

packers['array'] = function(buffer, tbl, n)
  if n <= 0x0F then
    buffer[#buffer + 1] = char(0x90 + n)
  elseif n <= 0xFFFF then
    buffer[#buffer + 1] = char(0xDC, floor(n / 0x100), n % 0x100)
  else
    buffer[#buffer + 1] = char(0xDD, floor(n / 0x1000000), floor(n / 0x10000) % 0x100, floor(n / 0x100) % 0x100, n % 0x100)
  end
  for i = 1, n do
    local v = tbl[i]
    packers[type(v)](buffer, v)
  end
end

packers['table'] = function(buffer, tbl)
  local is_map, n, max = false, 0, 0
  for k in pairs(tbl) do
    if type(k) == 'number' and k > 0 then
      if k > max then max = k end
    else
      is_map = true
    end
    n = n + 1
  end
  if max ~= n then is_map = true end
  if is_map then packers['map'](buffer, tbl, n) else packers['array'](buffer, tbl, n) end
end

packers['unsigned'] = function(buffer, n)
  if n <= 0x7F then
    buffer[#buffer + 1] = char(n)
  elseif n <= 0xFF then
    buffer[#buffer + 1] = char(0xCC, n)
  elseif n <= 0xFFFF then
    buffer[#buffer + 1] = char(0xCD, floor(n / 0x100), n % 0x100)
  elseif n <= 4294967295.0 then
    buffer[#buffer + 1] = char(0xCE, floor(n / 0x1000000), floor(n / 0x10000) % 0x100, floor(n / 0x100) % 0x100, n % 0x100)
  else
    buffer[#buffer + 1] = char(0xCF, 0, floor(n / 0x1000000000000) % 0x100, floor(n / 0x10000000000) % 0x100,
      floor(n / 0x100000000) % 0x100, floor(n / 0x1000000) % 0x100, floor(n / 0x10000) % 0x100,
      floor(n / 0x100) % 0x100, n % 0x100)
  end
end

packers['signed'] = function(buffer, n)
  if n >= 0 then
    packers['unsigned'](buffer, n)
  elseif n >= -0x20 then
    buffer[#buffer + 1] = char(0x100 + n)
  elseif n >= -0x80 then
    buffer[#buffer + 1] = char(0xD0, 0x100 + n)
  elseif n >= -0x8000 then
    n = 0x10000 + n
    buffer[#buffer + 1] = char(0xD1, floor(n / 0x100), n % 0x100)
  else
    n = 4294967296.0 + n
    buffer[#buffer + 1] = char(0xD2, floor(n / 0x1000000), floor(n / 0x10000) % 0x100, floor(n / 0x100) % 0x100, n % 0x100)
  end
end

-- doubles via frexp-free decomposition (fixed-point: encode as float64 of value*1, only finite values)
local function frexp(x)
  if x == 0 then return 0, 0 end
  local e = floor(math.log(math.abs(x)) / math.log(2)) + 1
  local mant = x / 2 ^ e
  if math.abs(mant) >= 1 then mant = mant / 2; e = e + 1 end
  if math.abs(mant) < 0.5 then mant = mant * 2; e = e - 1 end
  return mant, e
end

packers['double'] = function(buffer, n)
  local sign = 0
  if n < 0.0 then sign = 0x80; n = -n end
  local mant, expo = frexp(n)
  if mant ~= mant then
    buffer[#buffer + 1] = char(0xCB, 0xFF, 0xF8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00)
  elseif n == 0.0 then
    buffer[#buffer + 1] = char(0xCB, sign, 0, 0, 0, 0, 0, 0, 0)
  else
    expo = expo + 0x3FE
    mant = floor((mant * 2.0 - 1.0) * 2 ^ 52)
    buffer[#buffer + 1] = char(0xCB,
      sign + floor(expo / 0x10),
      (expo % 0x10) * 0x10 + floor(mant / 0x1000000000000),
      floor(mant / 0x10000000000) % 0x100,
      floor(mant / 0x100000000) % 0x100,
      floor(mant / 0x1000000) % 0x100,
      floor(mant / 0x10000) % 0x100,
      floor(mant / 0x100) % 0x100,
      mant % 0x100)
  end
end

packers['number'] = function(buffer, n)
  if floor(n) == n and n < 2 ^ 53 and n > -2 ^ 31 then
    packers['signed'](buffer, n)
  else
    packers['double'](buffer, n)
  end
end

function m.pack(data)
  local buffer = {}
  packers[type(data)](buffer, data)
  return tconcat(buffer)
end

-- unpacker
local unpackers
local function underflow(c, e)
  error("missing bytes")
end

local function unpack_cursor(c)
  local s, i, j = c.s, c.i, c.j
  if i > j then
    c:underflow(i)
    s, i, j = c.s, c.i, c.j
  end
  local val = byte(s, i)
  c.i = i + 1
  return unpackers[val](c, val)
end
m.unpack_cursor = unpack_cursor

local function unpack_str(c, n)
  local s, i, j = c.s, c.i, c.j
  local e = i + n - 1
  if e > j then c:underflow(e) end
  c.i = i + n
  return s:sub(i, e)
end

local function unpack_array(c, n)
  local t = {}
  for i = 1, n do t[i] = unpack_cursor(c) end
  return t
end

local function unpack_map(c, n)
  local t = {}
  for i = 1, n do
    local k = unpack_cursor(c)
    local val = unpack_cursor(c)
    if k == nil or k ~= k then k = m.sentinel end
    if k ~= nil then t[k] = val end
  end
  return t
end

local function unpack_uint(c, n)
  local s, i = c.s, c.i
  local e = i + n - 1
  if e > c.j then c:underflow(e) end
  local b = { byte(s, i, e) }
  local r = 0
  for k = 1, n do r = r * 256 + b[k] end
  c.i = i + n
  return r
end

local function unpack_int(c, n)
  local r = unpack_uint(c, n)
  local lim = 2 ^ (8 * n - 1)
  if r >= lim then r = r - 2 * lim end
  return r
end

local function unpack_double(c)
  local s, i = c.s, c.i
  if i + 7 > c.j then c:underflow(i + 7) end
  local b1, b2, b3, b4, b5, b6, b7, b8 = byte(s, i, i + 7)
  c.i = i + 8
  local sign = b1 > 0x7F
  local expo = (b1 % 0x80) * 0x10 + floor(b2 / 0x10)
  local mant = ((((((b2 % 0x10) * 0x100 + b3) * 0x100 + b4) * 0x100 + b5) * 0x100 + b6) * 0x100 + b7) * 0x100 + b8
  local n
  if mant == 0 and expo == 0 then
    n = 0.0
  elseif expo == 0x7FF then
    n = 0 / 0
  else
    n = (1.0 + mant / 2 ^ 52) * 2 ^ (expo - 0x3FF)
  end
  if sign then n = -n end
  return n
end

unpackers = setmetatable({
  [0xC0] = function() return nil end,
  [0xC2] = function() return false end,
  [0xC3] = function() return true end,
  [0xCB] = function(c) return unpack_double(c) end,
  [0xCC] = function(c) return unpack_uint(c, 1) end,
  [0xCD] = function(c) return unpack_uint(c, 2) end,
  [0xCE] = function(c) return unpack_uint(c, 4) end,
  [0xCF] = function(c) return unpack_uint(c, 8) end,
  [0xD0] = function(c) return unpack_int(c, 1) end,
  [0xD1] = function(c) return unpack_int(c, 2) end,
  [0xD2] = function(c) return unpack_int(c, 4) end,
  [0xD9] = function(c) return unpack_str(c, unpack_uint(c, 1)) end,
  [0xDA] = function(c) return unpack_str(c, unpack_uint(c, 2)) end,
  [0xDB] = function(c) return unpack_str(c, unpack_uint(c, 4)) end,
  [0xDC] = function(c) return unpack_array(c, unpack_uint(c, 2)) end,
  [0xDD] = function(c) return unpack_array(c, unpack_uint(c, 4)) end,
  [0xDE] = function(c) return unpack_map(c, unpack_uint(c, 2)) end,
  [0xDF] = function(c) return unpack_map(c, unpack_uint(c, 4)) end,
}, {
  __index = function(t, k)
    if k < 0xC0 then
      if k < 0x80 then
        return function(c, val) return val end
      elseif k < 0x90 then
        return function(c, val) return unpack_map(c, val % 0x10) end
      elseif k < 0xA0 then
        return function(c, val) return unpack_array(c, val % 0x10) end
      else
        return function(c, val) return unpack_str(c, val % 0x20) end
      end
    elseif k > 0xDF then
      return function(c, val) return val - 0x100 end
    else
      return function() error("unpack '" .. string.format('%#x', k) .. "' is unimplemented") end
    end
  end
})

function m.unpack(s)
  local cursor = { s = s, i = 1, j = #s, underflow = underflow }
  local data = unpack_cursor(cursor)
  if cursor.i <= cursor.j then error("extra bytes") end
  return data
end

-- Workload
local N = tonumber(arg and arg[1]) or 150
local function make(i)
  return {
    id = i, neg = -i * 37, big = i * 100003, huge = 5000000000 + i, ratio = i / 7, name = "player_" .. i,
    long = string.rep("abc", 20 + i % 90), flags = { true, false, i % 2 == 0 },
    stats = { hp = 100 - i % 50, mp = i % 30, xp = i * i },
    inv = { "sword", "shield", "potion", i, -i, i + 0.5 },
  }
end
local data = {}
for i = 1, 300 do data[i] = make(i) end

local bytes, sum, h = 0, 0, 0
local s
for r = 1, N do
  s = m.pack(data)
  bytes = bytes + #s
  local back = m.unpack(s)
  for i = 1, #back, 3 do
    local e = back[i]
    sum = sum + e.id + e.neg + e.stats.xp + #e.inv + #e.long + (e.flags[3] and 1 or 0)
    if math.abs(e.ratio - i / 7) > 1e-12 then sum = sum + 1000000 end
  end
end
for i = 1, #s, 3 do h = (h * 31 + s:byte(i)) % 1000000007 end
print("bytes", bytes, "sum", sum, "hash", h)
local one = m.unpack(m.pack({ a = -1.5, b = 3.25e-7, c = -200000, d = { 1, 2, { "x" } } }))
print(string.format("%.17g %.17g %d %s", one.a, one.b, one.c, one.d[3][1]))
