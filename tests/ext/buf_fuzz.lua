-- Truncated and mutated encodings (fixed seed): decode must either return a value or raise an error, never crash or read
-- out of bounds; and LuaJIT and rnlua must agree on which, with the same message and the same value.
local C = dofile("_common.lua")
local show, norm = C.show, C.norm
local buf = require("string.buffer")
local rnd0 = C.rng(777)
-- byte 0x12 (a complex number in LuaJIT's format) is never generated: rnlua has no complex numbers
local function rnd(n) local v = rnd0(n) if n == 256 and v == 0x13 then v = 0x14 end return v end
pcall(require, "ffi") -- LuaJIT decodes the int64/uint64 tags (0x10, 0x11) to cdata only with the ffi library loaded; rnlua to integers

local seeds = {
  { 1, 2, 3 }, { x = 1 }, { "abc", { 1, 2 }, { y = { z = true } } }, { 1.5, -2, "s", false, {} },
  { list = { 1, 2, 3, 4, 5 } }, (function() local t = { 1, 2 } t[0] = 5 return t end)(),
  ("x"):rep(300), { ("y"):rep(40), ("z"):rep(230) },
}
local N = tonumber(os.getenv("BUF_FUZZ_N")) or 60 -- mutants per seed (the checked-in expectation uses the default)
local function try(label, s)
  if os.getenv("BUF_FUZZ_HEX") then io.write(C.hex(s), "  ") end
  local ok, r = pcall(buf.decode, s)
  if ok then print(label, "ok", show(r)) else print(label, "err", norm(r)) end
end

for si, v in ipairs(seeds) do
  local s = buf.encode(v)
  for n = 0, #s - 1 do try("trunc " .. si .. "/" .. n, s:sub(1, n)) end
  for k = 1, N do
    local m = { s:byte(1, -1) }
    for j = 1, rnd(3) do m[rnd(#m)] = rnd(256) - 1 end
    local ms = {}
    for i = 1, #m do ms[i] = string.char(m[i]) end
    local str = table.concat(ms)
    if rnd(4) == 1 then str = str:sub(1, rnd(#str)) end
    -- huge counts would make the oracle allocate a lot: skip inputs that decode to tables with giant presize
    try("mut " .. si .. "/" .. k, str)
  end
end
-- random bytes
for k = 1, N * 2 + 30 do
  local t = {}
  for i = 1, rnd(24) do t[i] = string.char(rnd(256) - 1) end
  try("rand " .. k, table.concat(t))
end
