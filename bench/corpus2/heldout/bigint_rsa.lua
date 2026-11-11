-- bigint_rsa: imitates a hobby bignum + toy RSA script: base-10000 little-endian digit arrays,
-- schoolbook multiply, long division, modular exponentiation, Miller-Rabin, key generation.
-- small: N=1
local N = tonumber(arg and arg[1]) or 12
local seed = 123457
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end
local BASE = 10000
local function trim(a)
  while #a > 0 and a[#a] == 0 do a[#a] = nil end
  return a
end

local function from_int(n)
  local a = {}
  while n > 0 do
    a[#a + 1] = n % BASE
    n = math.floor(n / BASE)
  end
  return a
end
local function from_string(s)
  local a = {}
  local i = #s
  while i >= 1 do
    local j = math.max(1, i - 3)
    a[#a + 1] = tonumber(s:sub(j, i))
    i = j - 1
  end
  return trim(a)
end

local function to_string(a)
  if #a == 0 then return "0" end
  local parts = { string.format("%d", a[#a]) }
  for i = #a - 1, 1, -1 do parts[#parts + 1] = string.format("%04d", a[i]) end
  return table.concat(parts)
end
local function copy(a)
  local c = {}
  for i = 1, #a do c[i] = a[i] end
  return c
end
local function cmp(a, b)
  if #a ~= #b then return #a < #b and -1 or 1 end
  for i = #a, 1, -1 do
    if a[i] ~= b[i] then return a[i] < b[i] and -1 or 1 end
  end
  return 0
end
local function add(a, b)
  local r, carry = {}, 0
  for i = 1, math.max(#a, #b) do
    local s = (a[i] or 0) + (b[i] or 0) + carry
    if s >= BASE then r[i] = s - BASE; carry = 1 else r[i] = s; carry = 0 end
  end
  if carry > 0 then r[#r + 1] = carry end
  return r
end
local function sub(a, b)            -- requires a >= b
  local r, borrow = {}, 0
  for i = 1, #a do
    local d = a[i] - (b[i] or 0) - borrow
    if d < 0 then r[i] = d + BASE; borrow = 1 else r[i] = d; borrow = 0 end
  end
  return trim(r)
end

local function mul_small(a, m)
  if m == 0 or #a == 0 then return {} end
  local r, carry = {}, 0
  for i = 1, #a do
    local p = a[i] * m + carry
    r[i] = p % BASE
    carry = math.floor(p / BASE)
  end
  while carry > 0 do
    r[#r + 1] = carry % BASE
    carry = math.floor(carry / BASE)
  end
  return r
end
local function divmod_small(a, m)
  local q, rem = {}, 0
  for i = #a, 1, -1 do
    local cur = rem * BASE + a[i]
    q[i] = math.floor(cur / m)
    rem = cur % m
  end
  return trim(q), rem
end
local function mul(a, b)
  if #a == 0 or #b == 0 then return {} end
  local r = {}
  for i = 1, #a + #b do r[i] = 0 end
  for i = 1, #a do
    local carry, ai = 0, a[i]
    for j = 1, #b do
      local cur = r[i + j - 1] + ai * b[j] + carry
      carry = math.floor(cur / BASE)
      r[i + j - 1] = cur % BASE
    end
    local k = i + #b
    while carry > 0 do
      local cur = r[k] + carry
      carry = math.floor(cur / BASE)
      r[k] = cur % BASE
      k = k + 1
    end
  end
  return trim(r)
end
-- long division, one base-10000 quotient digit at a time (digit found by binary search)
local function divmod(a, b)
  if #b == 0 then error("division by zero") end
  if cmp(a, b) < 0 then return {}, copy(a) end
  if #b == 1 then
    local q, r = divmod_small(a, b[1])
    return q, from_int(r)
  end
  local q, rem = {}, {}
  for i = #a, 1, -1 do
    table.insert(rem, 1, a[i])
    trim(rem)
    local lo, hi = 0, BASE - 1
    if cmp(rem, b) < 0 then hi = 0 end
    while lo < hi do
      local mid = math.floor((lo + hi + 1) / 2)
      if cmp(mul_small(b, mid), rem) <= 0 then lo = mid else hi = mid - 1 end
    end
    if lo > 0 then rem = sub(rem, mul_small(b, lo)) end
    q[i] = lo
  end
  return trim(q), rem
end
local function mod(a, m)
  local _, r = divmod(a, m)
  return r
end

local ONE, TWO = from_int(1), from_int(2)

local function is_even(a) return #a == 0 or a[1] % 2 == 0 end

local function modexp(base, exp, m)
  local bits, e = {}, copy(exp)
  while #e > 0 do
    local r
    e, r = divmod_small(e, 2)
    bits[#bits + 1] = r
  end
  local result = ONE
  for i = #bits, 1, -1 do
    result = mod(mul(result, result), m)
    if bits[i] == 1 then result = mod(mul(result, base), m) end
  end
  return result
end
local SMALL_PRIMES = { 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47 }
local witness_calls = 0

local function is_probable_prime(n)
  if is_even(n) then return cmp(n, TWO) == 0 end
  for _, p in ipairs(SMALL_PRIMES) do
    local _, r = divmod_small(n, p)
    if r == 0 then return cmp(n, from_int(p)) == 0 end
  end
  local nm1 = sub(n, ONE)
  local d, s = copy(nm1), 0
  while is_even(d) do d = divmod_small(d, 2); s = s + 1 end
  for _, a in ipairs({ 2, 3, 5, 7 }) do
    witness_calls = witness_calls + 1
    local x = modexp(from_int(a), d, n)
    if cmp(x, ONE) ~= 0 and cmp(x, nm1) ~= 0 then
      local composite = true
      for _ = 1, s - 1 do
        x = mod(mul(x, x), n)
        if cmp(x, nm1) == 0 then composite = false; break end
      end
      if composite then return false end
    end
  end
  return true
end

local function random_big(limbs)
  local a = {}
  for i = 1, limbs do a[i] = rnd(BASE) - 1 end
  if a[limbs] == 0 then a[limbs] = rnd(BASE - 1) end
  return a
end
local function random_prime(limbs)
  local tries = 0
  while true do
    tries = tries + 1
    local c = random_big(limbs)
    if is_even(c) then c[1] = c[1] + 1 end
    if is_probable_prime(c) then return c, tries end
  end
end
local function mod_sub(a, b, m)     -- (a - b) mod m for a, b < m
  if cmp(a, b) >= 0 then return sub(a, b) end
  return sub(add(a, m), b)
end

local function mod_inverse(e, phi)
  -- extended Euclid keeping only the coefficient of e, reduced mod phi
  local r0, r1 = copy(phi), mod(e, phi)
  local t0, t1 = {}, ONE
  while #r1 > 0 do
    local q, r2 = divmod(r0, r1)
    local t2 = mod_sub(t0, mod(mul(q, t1), phi), phi)
    r0, r1, t0, t1 = r1, r2, t1, t2
  end
  if cmp(r0, ONE) ~= 0 then return nil end
  return t0
end
local function make_key(limbs)
  local E = from_int(65537)
  while true do
    local p = random_prime(limbs)
    local q = random_prime(limbs)
    if cmp(p, q) ~= 0 then
      local phi = mul(sub(p, ONE), sub(q, ONE))
      local d = mod_inverse(E, phi)
      if d then return { n = mul(p, q), e = E, d = d, p = p, q = q } end
    end
  end
end

-- bytes <-> bignum, 5 bytes per block
local function bytes_to_big(s)
  local a = {}
  for i = 1, #s do a = add(mul_small(a, 256), from_int(s:byte(i))) end
  return a
end
local function big_to_bytes(a, len)
  local out = {}
  for i = len, 1, -1 do
    local r
    a, r = divmod_small(a, 256)
    out[i] = string.char(r)
  end
  return table.concat(out)
end
local function encrypt(key, msg)
  local blocks = {}
  for i = 1, #msg, 5 do
    local chunk = msg:sub(i, i + 4)
    blocks[#blocks + 1] = { modexp(bytes_to_big(chunk), key.e, key.n), #chunk }
  end
  return blocks
end
local function decrypt(key, blocks)
  local out = {}
  for _, b in ipairs(blocks) do out[#out + 1] = big_to_bytes(modexp(b[1], key.d, key.n), b[2]) end
  return table.concat(out)
end
-- arithmetic sanity checks against known values
local fact = ONE
for i = 2, 25 do fact = mul_small(fact, i) end
print("25! =", to_string(fact))
local a, b = from_int(1), from_int(1)
for _ = 1, 150 do a, b = b, add(a, b) end
print("fib(151) =", to_string(a))
local pw = ONE
for _ = 1, 100 do pw = mul_small(pw, 2) end
print("2^100 =", to_string(pw))
local q, r = divmod(from_string("123456789012345678901234567890123"), from_string("987654321987"))
print("division:", to_string(q), to_string(r))
print("modexp:", to_string(modexp(from_int(4), from_int(13), from_int(497))))
local messages = { "attack at dawn", "hello, world", "lua bigint toy", "0123456789", "the quick brown fox" }
local total_ok, checksum, tries_total = 0, 0, 0
local ndigits = 0
for k = 1, N do
  local key = make_key(N <= 2 and 2 or 3)
  local ok = true
  for mi = 1, 3 do
    local msg = messages[(k + mi) % #messages + 1]
    local blocks = encrypt(key, msg)
    local plain = decrypt(key, blocks)
    if plain ~= msg then ok = false end
    for _, blk in ipairs(blocks) do
      for i = 1, #blk[1] do checksum = (checksum * 7 + blk[1][i]) % 1000000007 end
    end
  end
  if ok then total_ok = total_ok + 1 end
  ndigits = ndigits + #to_string(key.n)
  if k <= 3 then
    print(string.format("key %d: n=%s e=%s", k, to_string(key.n), to_string(key.e)))
    print(string.format("       d=%s", to_string(key.d)))
    print(string.format("       p=%s q=%s", to_string(key.p), to_string(key.q)))
  end
end
print(string.format("keys=%d roundtrips ok=%d avg modulus digits=%.1f witness calls=%d checksum=%d", N, total_ok,
  ndigits / N, witness_calls, checksum))
-- known-prime / known-composite checks
local tests = { "1000003", "1000001", "999999000001", "2147483647", "2147483649" }
local flags = {}
for _, t in ipairs(tests) do flags[#flags + 1] = t .. "=" .. (is_probable_prime(from_string(t)) and "P" or "C") end
print(table.concat(flags, " "))
