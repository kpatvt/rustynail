-- Pure-Lua SHA-256 + base64 + hex (in the style of Egor Skriptunoff's sha2.lua / iskolbin base64):
-- the bit operations are picked per engine (LuaJIT `bit` library, native 5.3+ operators via load()),
-- the base64 codec is the 5.1-compatible arithmetic one with lookup tables.
local band, bor, bxor, bnot, rshift, lshift, rrotate
if bit and bit.band then  -- LuaJIT
  local bit = bit
  local tobit = bit.tobit
  band, bor, bxor, bnot = bit.band, bit.bor, bit.bxor, bit.bnot
  rshift, lshift = bit.rshift, bit.lshift
  local ror = bit.ror
  rrotate = function(x, n) return ror(x, n) end
  local norm = function(x) return x % 4294967296 end
  band = function(a, b) return norm(bit.band(a, b)) end
  bor = function(a, b) return norm(bit.bor(a, b)) end
  bxor = function(a, b, c) if c then return norm(bit.bxor(a, b, c)) end return norm(bit.bxor(a, b)) end
  bnot = function(a) return norm(bit.bnot(a)) end
  rshift = function(a, n) return norm(bit.rshift(a, n)) end
  lshift = function(a, n) return norm(bit.lshift(a, n)) end
  rrotate = function(a, n) return norm(ror(a, n)) end
else
  local ops = assert(load([[
    local M = 0xFFFFFFFF
    return function(a, b) return a & b end,
           function(a, b) return a | b end,
           function(a, b, c) if c then return a ~ b ~ c end return a ~ b end,
           function(a) return ~a & M end,
           function(a, n) return (a & M) >> n end,
           function(a, n) return (a << n) & M end,
           function(a, n) a = a & M; return ((a >> n) | (a << (32 - n))) & M end
  ]]))
  band, bor, bxor, bnot, rshift, lshift, rrotate = ops()
end

local char, byte, format, rep, sub = string.char, string.byte, string.format, string.rep, string.sub
local floor = math.floor

local k = {
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
}

local function num2s(l, n)
  local s = ""
  for i = 1, n do
    local rem = l % 256
    s = char(rem) .. s
    l = (l - rem) / 256
  end
  return s
end

local function s232num(s, i)
  local n = 0
  for j = i, i + 3 do n = n * 256 + byte(s, j) end
  return n
end

local function preprocess(msg, len)
  local extra = 64 - ((len + 9) % 64)
  len = num2s(8 * len, 8)
  msg = msg .. "\128" .. rep("\0", extra) .. len
  assert(#msg % 64 == 0)
  return msg
end

local w = {}
local function digestblock(msg, i, H)
  for j = 1, 16 do w[j] = s232num(msg, i + (j - 1) * 4) end
  for j = 17, 64 do
    local v = w[j - 15]
    local s0 = bxor(rrotate(v, 7), rrotate(v, 18), rshift(v, 3))
    v = w[j - 2]
    local s1 = bxor(rrotate(v, 17), rrotate(v, 19), rshift(v, 10))
    w[j] = (w[j - 16] + s0 + w[j - 7] + s1) % 4294967296
  end
  local a, b, c, d, e, f, g, h = H[1], H[2], H[3], H[4], H[5], H[6], H[7], H[8]
  for i = 1, 64 do
    local s0 = bxor(rrotate(a, 2), rrotate(a, 13), rrotate(a, 22))
    local maj = bxor(band(a, b), band(a, c), band(b, c))
    local t2 = s0 + maj
    local s1 = bxor(rrotate(e, 6), rrotate(e, 11), rrotate(e, 25))
    local ch = bxor(band(e, f), band(bnot(e), g))
    local t1 = h + s1 + ch + k[i] + w[i]
    h, g, f, e, d, c, b, a = g, f, e, (d + t1) % 4294967296, c, b, a, (t1 + t2) % 4294967296
  end
  H[1] = (H[1] + a) % 4294967296
  H[2] = (H[2] + b) % 4294967296
  H[3] = (H[3] + c) % 4294967296
  H[4] = (H[4] + d) % 4294967296
  H[5] = (H[5] + e) % 4294967296
  H[6] = (H[6] + f) % 4294967296
  H[7] = (H[7] + g) % 4294967296
  H[8] = (H[8] + h) % 4294967296
end

local function sha256(msg)
  msg = preprocess(msg, #msg)
  local H = { 0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19 }
  for i = 1, #msg, 64 do digestblock(msg, i, H) end
  local out = {}
  for i = 1, 8 do out[i] = format("%08x", H[i]) end
  return table.concat(out)
end

local function hex2bin(h)
  return (h:gsub("%x%x", function(c) return char(tonumber(c, 16)) end))
end

-- base64 (arithmetic, 5.1 compatible)
local b64chars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
local enc, dec = {}, {}
for i = 1, 64 do
  local c = sub(b64chars, i, i)
  enc[i - 1] = c
  dec[byte(c)] = i - 1
end

local function b64encode(s)
  local out, n = {}, 0
  local len = #s
  for i = 1, len - 2, 3 do
    local a, b, c = byte(s, i, i + 2)
    local v = a * 65536 + b * 256 + c
    n = n + 1
    out[n] = enc[floor(v / 262144)] .. enc[floor(v / 4096) % 64] .. enc[floor(v / 64) % 64] .. enc[v % 64]
  end
  local rem = len % 3
  if rem == 1 then
    local a = byte(s, len)
    n = n + 1
    out[n] = enc[floor(a / 4)] .. enc[(a % 4) * 16] .. "=="
  elseif rem == 2 then
    local a, b = byte(s, len - 1, len)
    local v = a * 256 + b
    n = n + 1
    out[n] = enc[floor(v / 1024)] .. enc[floor(v / 16) % 64] .. enc[(v % 16) * 4] .. "="
  end
  return table.concat(out)
end

local function b64decode(s)
  s = s:gsub("[^%w%+/=]", "")
  local out, n = {}, 0
  for i = 1, #s, 4 do
    local a, b, c, d = byte(s, i, i + 3)
    local v = dec[a] * 262144 + dec[b] * 4096 + (dec[c] or 0) * 64 + (dec[d] or 0)
    n = n + 1
    if d == 61 then
      if c == 61 then
        out[n] = char(floor(v / 65536))
      else
        out[n] = char(floor(v / 65536), floor(v / 256) % 256)
      end
    else
      out[n] = char(floor(v / 65536), floor(v / 256) % 256, v % 256)
    end
  end
  return table.concat(out)
end

-- HMAC-SHA256
local function hmac_sha256(key, msg)
  if #key > 64 then key = hex2bin(sha256(key)) end
  key = key .. rep("\0", 64 - #key)
  local ipad = key:gsub(".", function(c) return char(bxor(byte(c), 0x36)) end)
  local opad = key:gsub(".", function(c) return char(bxor(byte(c), 0x5c)) end)
  return sha256(opad .. hex2bin(sha256(ipad .. msg)))
end

local N = tonumber(arg and arg[1]) or 600
print(sha256(""))
print(sha256("abc"))
print(sha256("The quick brown fox jumps over the lazy dog"))
print(hmac_sha256("key", "The quick brown fox jumps over the lazy dog"))
print(b64encode("Many hands make light work."), b64decode("TWFueSBoYW5kcyBtYWtlIGxpZ2h0IHdvcmsu"))

local parts = {}
for i = 1, 2000 do parts[i] = char(i % 256) end
local blob = table.concat(parts)
local digest, okc = "", 0
for r = 1, N do
  local msg = blob .. r
  digest = sha256(msg .. digest)
  local e = b64encode(msg)
  local d = b64decode(e)
  if d == msg then okc = okc + 1 end
  digest = hmac_sha256(digest, e:sub(1, 100))
end
print("final", digest, "ok", okc)
