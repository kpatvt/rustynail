-- Arbitrary-precision integer arithmetic on arrays of base-10^7 limbs: add, sub, mul, divmod
-- by small int, compare, tostring; factorials, Fibonacci, digits of e and pi. small: N=2
local N = tonumber(arg and arg[1]) or 80

local BASE = 10000000
local BASE_DIGITS = 7

local Big = {}
Big.__index = Big

local function trim(a)
  while #a > 1 and a[#a] == 0 do a[#a] = nil end
  return a
end

function Big.new(n)
  local self = setmetatable({}, Big)
  n = n or 0
  if n == 0 then self[1] = 0 end
  while n > 0 do
    self[#self + 1] = n % BASE
    n = math.floor(n / BASE)
  end
  return self
end

function Big.fromString(s)
  local self = setmetatable({}, Big)
  local i = #s
  while i >= 1 do
    local from = math.max(1, i - BASE_DIGITS + 1)
    self[#self + 1] = tonumber(s:sub(from, i))
    i = from - 1
  end
  return trim(self)
end

function Big.__tostring(a)
  local parts = { tostring(a[#a]) }
  for i = #a - 1, 1, -1 do
    parts[#parts + 1] = string.format("%07d", a[i])
  end
  return table.concat(parts)
end

function Big.cmp(a, b)
  if #a ~= #b then return #a < #b and -1 or 1 end
  for i = #a, 1, -1 do
    if a[i] ~= b[i] then return a[i] < b[i] and -1 or 1 end
  end
  return 0
end

function Big.__eq(a, b) return Big.cmp(a, b) == 0 end
function Big.__lt(a, b) return Big.cmp(a, b) < 0 end
function Big.__le(a, b) return Big.cmp(a, b) <= 0 end

function Big.__add(a, b)
  local r, carry = setmetatable({}, Big), 0
  for i = 1, math.max(#a, #b) do
    local s = (a[i] or 0) + (b[i] or 0) + carry
    if s >= BASE then r[i] = s - BASE; carry = 1 else r[i] = s; carry = 0 end
  end
  if carry > 0 then r[#r + 1] = carry end
  return r
end

function Big.__sub(a, b)
  assert(a >= b, "negative result")
  local r, borrow = setmetatable({}, Big), 0
  for i = 1, #a do
    local d = a[i] - (b[i] or 0) - borrow
    if d < 0 then d = d + BASE; borrow = 1 else borrow = 0 end
    r[i] = d
  end
  return trim(r)
end

function Big.__mul(a, b)
  if type(a) == "number" then a = Big.new(a) end
  if type(b) == "number" then b = Big.new(b) end
  local r = setmetatable({}, Big)
  for i = 1, #a + #b do r[i] = 0 end
  for i = 1, #a do
    local carry, ai = 0, a[i]
    if ai ~= 0 then
      for j = 1, #b do
        local cur = r[i + j - 1] + ai * b[j] + carry
        carry = math.floor(cur / BASE)
        r[i + j - 1] = cur - carry * BASE
      end
      local k = i + #b
      while carry > 0 do
        local cur = r[k] + carry
        carry = math.floor(cur / BASE)
        r[k] = cur - carry * BASE
        k = k + 1
      end
    end
  end
  return trim(r)
end

-- divide by a small integer (< BASE), returns quotient and remainder
function Big.divmodSmall(a, d)
  local q, rem = setmetatable({}, Big), 0
  for i = #a, 1, -1 do
    local cur = rem * BASE + a[i]
    local qi = math.floor(cur / d)
    q[i] = qi
    rem = cur - qi * d
  end
  return trim(q), rem
end

function Big.mulSmall(a, m)
  local r, carry = setmetatable({}, Big), 0
  for i = 1, #a do
    local cur = a[i] * m + carry
    carry = math.floor(cur / BASE)
    r[i] = cur - carry * BASE
  end
  while carry > 0 do
    r[#r + 1] = carry % BASE
    carry = math.floor(carry / BASE)
  end
  return trim(r)
end

local function digitSum(s)
  local sum = 0
  for i = 1, #s do sum = sum + s:byte(i) - 48 end
  return sum
end

local function checksum(s)
  local c = 0
  for i = 1, #s do c = (c * 31 + s:byte(i)) % 1000000007 end
  return c
end

local function describe(label, big)
  local s = tostring(big)
  print(string.format("%-14s digits=%5d sum=%6d crc=%10d head=%s tail=%s", label, #s, digitSum(s), checksum(s),
    s:sub(1, 12), s:sub(-6)))
end

-- Factorial -----------------------------------------------------------------------------------
local function factorial(n)
  local r = Big.new(1)
  for i = 2, n do r = r:mulSmall(i) end
  return r
end

-- Fibonacci by repeated addition and by fast doubling-free matrix-less loop -----------------------------
local function fib(n)
  local a, b = Big.new(0), Big.new(1)
  for _ = 1, n do a, b = b, a + b end
  return a
end

-- Binary-splitting-free e: sum of 1/k! scaled by 10^digits using small division ------------------
local function eDigits(digits)
  local limbs = math.floor(digits / BASE_DIGITS) + 2
  local one = Big.new(1)
  for _ = 1, limbs do table.insert(one, 1, 0) end
  local term, total = one, one
  local k = 1
  while true do
    term = term:divmodSmall(k)
    if #term == 1 and term[1] == 0 then break end
    total = total + term
    k = k + 1
  end
  local s = tostring(total)
  return s:sub(1, 1) .. "." .. s:sub(2, digits + 1)
end

-- Rabinowitz-Wagon pi spigot (integer arrays) --------------------------------------------------------------
local function piDigits(count)
  local len = math.floor(count * 10 / 3) + 1
  local a = {}
  for i = 1, len do a[i] = 2 end
  local out, nines, predigit = {}, 0, 0
  for j = 1, count do
    local q = 0
    for i = len, 1, -1 do
      local x = 10 * a[i] + q * i
      a[i] = x % (2 * i - 1)
      q = math.floor(x / (2 * i - 1))
    end
    a[1] = q % 10
    q = math.floor(q / 10)
    if q == 9 then
      nines = nines + 1
    elseif q == 10 then
      out[#out + 1] = tostring(predigit + 1)
      for _ = 1, nines do out[#out + 1] = "0" end
      predigit, nines = 0, 0
    else
      if j > 1 then out[#out + 1] = tostring(predigit) end
      for _ = 1, nines do out[#out + 1] = "9" end
      predigit, nines = q, 0
    end
  end
  out[#out + 1] = tostring(predigit)
  return table.concat(out)
end

-- Run -----------------------------------------------------------------------------------------------------------
local f = factorial(20)
print("20! = " .. tostring(f))
describe("N*20 !", factorial(N * 20))
describe("fib(N*40)", fib(N * 40))

local x = Big.fromString("123456789012345678901234567890")
local y = Big.fromString("987654321098765432109876543210")
print(tostring(x + y), tostring(y - x), tostring(x * y))
print(x < y, x == Big.fromString("123456789012345678901234567890"), y <= x)
local q, r = y:divmodSmall(97)
print(tostring(q), r)
print(tostring((y - x) * Big.new(0)), tostring(Big.new(9999999) + Big.new(1)))

-- (a+b)^2 == a^2 + 2ab + b^2 identity over a few big numbers
local ok = 0
for i = 1, N * 3 do
  local a, b = fib(30 + i * 7), factorial(15 + i)
  local lhs = (a + b) * (a + b)
  local rhs = a * a + a * b * 2 + b * b
  if lhs == rhs then ok = ok + 1 end
end
print("identity holds: " .. ok .. "/" .. N * 3)

local e = eDigits(N * 40)
print("e = " .. e:sub(1, 40) .. " (" .. #e .. " chars, crc " .. checksum(e) .. ")")
local pi = piDigits(N * 20)
print("pi = " .. pi:sub(1, 1) .. "." .. pi:sub(2, 40) .. " (crc " .. checksum(pi) .. ")")
