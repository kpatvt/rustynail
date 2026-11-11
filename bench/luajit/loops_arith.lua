-- LuaJIT variant of bench/programs/loops_arith.lua (H1: uses the bit.* library faithfully).
-- The 5.4/5.5 program does  i >> 2,  x ~ (i * 2654435761 & 0xffffffff)  on 64-bit integers.
-- LuaJIT has no integer subtype; bit.* works on the low 32 bits of a double (exact below 2^51).
--   * i >> 2 (i < 2^31, positive)  ==  bit.rshift(i, 2)                       (one real shift)
--   * i * 2654435761 does not fit a double exactly (i up to 3e7 -> 8e16 > 2^53), but only the low 32 bits are needed:
--     low32(i * K) = low32(i * KL) + (low32(i * KH) << 16)   with K = KH * 65536 + KL (KL, KH < 2^16),
--     every partial product < 2^53, so the result equals the integer program's bit for bit.
--   * x ~ m  ==  bit.bxor(x, m); bit.* results are signed 32-bit, so x is normalised to [0, 2^32) once, at the end
--     (the xor of the signed images is the signed image of the xor).
-- Work per iteration is the same as in the integer program (one shift / two multiplies + shift + add + xor).
local rshift, bxor, tobit, lshift = bit.rshift, bit.bxor, bit.tobit, bit.lshift
local n = tonumber(arg and arg[1]) or 60000000
local s, f = 0, 0.0
for i = 1, n do s = s + i % 7 * 3 - rshift(i, 2); f = f + i * 0.5 end
local x = 0
local KH, KL = 40503, 2654435761 % 65536   -- 2654435761 = 40503 * 65536 + KL
for i = 1, math.floor(n / 2) do
  local m = tobit(i * KL) + lshift(i * KH, 16)
  x = bxor(x, m)
end
if x < 0 then x = x + 4294967296 end
local w, k = 0, 0
local lim = math.floor(n / 3)
while k < lim do k = k + 1; if k % 3 == 0 then w = w + k else w = w - 1 end end
print(string.format("%d", s), string.format("%.1f", f), string.format("%d", x), string.format("%d", w))
