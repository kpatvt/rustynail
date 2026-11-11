
local a = {
  1//0 == nil, 0.0/0.0, -(0.0), 0.0 * -1, 1e308 * 10, -1e308 * 10,
  math.maxinteger + 1, math.mininteger - 1, math.maxinteger * 2,
  1 << 63, 1 << 64, 1 >> 1, -1 >> 1, -1 << 1, 3 & 5.0, 3 | 4.5, ~0, ~5.0,
  7 // 2, -7 // 2, 7 % -3, -7 % 3, 7.5 // 2, 7.5 % 2, 2^-1, 8 // 0.0, -8 // 0.0,
  0x10 + 0xA, 0xff // 3, "10" + 1, 10 .. 20, 2^63 == math.mininteger,
  1e15 + 0.5, 3 % math.huge, -3 % math.huge, 3 % -math.huge,
}
return a
