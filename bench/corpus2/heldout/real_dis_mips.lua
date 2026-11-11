-- REAL CODE driver: LuaJIT's MIPS disassembler (jit/dis_mips.lua, jit/dis_mipsel.lua, (c) Mike Pall, MIT license),
-- taken from the system LuaJIT package and ported from its extended syntax to standard Lua (vendor/jit/, see MANIFEST).
-- Workload: disassemble deterministic pseudo-random MIPS32 words, big- and little-endian.
-- usage: real_dis_mips.lua [N]  (small: N=0.1)
local dir = (arg and arg[0] or ""):match("^(.*/)") or "./"
package.path = dir .. "../vendor/?.lua;" .. package.path
if not pcall(require, "bit") then
  package.preload.bit = function()
    local function tohex(x, n)
      n = n or 8
      local up = n < 0
      if up then n = -n end
      x = x % 4294967296
      local s = string.format(up and "%08X" or "%08x", x)
      return s:sub(9 - n)
    end
    local function ror(x, n)
      x = x % 4294967296; n = n % 32
      local r = (x >> n) | ((x << (32 - n)) & 0xffffffff)
      if r >= 0x80000000 then r = r - 4294967296 end
      return r
    end
    return { tohex = tohex, ror = ror }
  end
end

local N = tonumber(arg and arg[1]) or 20

local seed = 4242
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n
end

-- Primary opcodes weighted like ordinary compiled code (loads/stores, addiu, branches, SPECIAL ALU ops, COP1).
local common = { 0, 0, 0, 0, 9, 9, 9, 35, 35, 43, 43, 15, 4, 5, 3, 2, 1, 6, 7, 12, 13, 10, 49, 53, 57, 61, 17, 28, 31, 32, 36, 40, 41 }

local function gen_code(nwords, little)
  local s = {}
  for i = 1, nwords do
    local op
    if rnd(10) < 7 then
      op = common[rnd(#common) + 1] * 67108864 + rnd(67108864)
    else
      op = rnd(65536) * 65536 + rnd(65536)
    end
    local b0, b1, b2, b3 = math.floor(op / 16777216) % 256, math.floor(op / 65536) % 256, math.floor(op / 256) % 256, op % 256
    if little then s[i] = string.char(b3, b2, b1, b0) else s[i] = string.char(b0, b1, b2, b3) end
  end
  return table.concat(s)
end

local function run(modname, little, blocks, nwords)
  local dis = require(modname)
  local lines, chars, sum = 0, 0, 0
  local mnem, sample = {}, {}
  local function out(s)
    lines = lines + 1
    chars = chars + #s
    local m = s:match("^%x+%s+%x+%s+([%a][%w%.]*)")
    if m then mnem[m] = (mnem[m] or 0) + 1 end
    sum = (sum * 31 + #s + (s:byte(-2) or 0)) % 1000000007
    if lines % 5000 == 1 and #sample < 8 then sample[#sample + 1] = s end
  end
  for b = 1, blocks do
    local code = gen_code(nwords, little)
    local ctx = dis.create(code, 0x10000000 + b * 0x4000, out)
    ctx:disass()
  end
  local names = {}
  for k in pairs(mnem) do names[#names + 1] = k end
  table.sort(names, function(a, b)
    if mnem[a] ~= mnem[b] then return mnem[a] > mnem[b] end
    return a < b
  end)
  print(string.format("%s: %d lines, %d chars, checksum %d, %d mnemonics", modname, lines, chars, sum, #names))
  for i = 1, math.min(12, #names) do
    io.write(string.format("  %-10s %d\n", names[i], mnem[names[i]]))
  end
  for _, s in ipairs(sample) do io.write("  | ", s) end
end

run("jit.dis_mips", false, math.max(1, math.floor(5 * N)), 2000)
run("jit.dis_mipsel", true, math.max(1, math.floor(5 * N)), 2000)
