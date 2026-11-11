-- REAL CODE driver: LuaJIT's x86/x64 disassembler (jit/dis_x86.lua, jit/dis_x64.lua, (c) Mike Pall, MIT license),
-- taken from the system LuaJIT package and desugared from its extended syntax to standard Lua (vendor/jit/, see MANIFEST).
-- Workload: disassemble deterministic pseudo-random x86 code (biased towards common encodings) in 32-bit and 64-bit mode.
-- usage: real_dis_x86.lua [N] [mode]  (mode: 32, 64 or both; small: N=0.1)
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

local N = tonumber(arg and arg[1]) or 15
local MODE = arg and arg[2] or "both"

local seed = 12345
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n
end

-- Common encodings (prefix bytes followed by "m" = random modrm, "i1"/"i4" = immediates).
local templates = {
  { 0x89, "m" }, { 0x8b, "m" }, { 0x01, "m" }, { 0x29, "m" }, { 0x31, "m" }, { 0x39, "m" }, { 0x85, "m" },
  { 0x8d, "m" }, { 0x48, 0x89, "m" }, { 0x48, 0x8b, "m" }, { 0x48, 0x8d, "m" }, { 0x48, 0x83, "m", "i1" },
  { 0x83, "m", "i1" }, { 0x81, "m", "i4" }, { 0xe8, "i4" }, { 0xe9, "i4" }, { 0xeb, "i1" }, { 0x74, "i1" },
  { 0x75, "i1" }, { 0x0f, 0x84, "i4" }, { 0x0f, 0x8e, "i4" }, { 0x50 }, { 0x53 }, { 0x55 }, { 0x5d }, { 0x5b },
  { 0xc3 }, { 0x90 }, { 0xb8, "i4" }, { 0xc7, "m", "i4" }, { 0xf2, 0x0f, 0x10, "m" }, { 0xf2, 0x0f, 0x58, "m" },
  { 0xf2, 0x0f, 0x59, "m" }, { 0x66, 0x0f, 0x6f, "m" }, { 0x0f, 0xb6, "m" }, { 0x0f, 0xaf, "m" }, { 0xff, "m" },
  { 0xc1, "m", "i1" }, { 0xd3, "m" }, { 0x0f, 0x1f, "m" }, { 0x66, 0x90 }, { 0x4c, 0x8b, "m" }, { 0x41, 0xff, "m" },
  { 0xc5, 0xf8, 0x77 }, { 0xc5, 0xfb, 0x10, "m" }, { 0xc4, 0xe2, 0x79, 0x18, "m" }, { 0xf3, 0x48, 0xab },
}

local function gen_code(len)
  local out = {}
  while #out < len do
    if rnd(10) < 8 then
      local t = templates[rnd(#templates) + 1]
      for _, b in ipairs(t) do
        if b == "m" then
          local modrm = rnd(256)
          out[#out + 1] = modrm
          local mod, rm = math.floor(modrm / 64), modrm % 8
          if mod ~= 3 and rm == 4 then out[#out + 1] = rnd(256) end
          if mod == 1 then out[#out + 1] = rnd(256)
          elseif mod == 2 or (mod == 0 and rm == 5) then
            for _ = 1, 4 do out[#out + 1] = rnd(256) end
          end
        elseif b == "i1" then out[#out + 1] = rnd(256)
        elseif b == "i4" then for _ = 1, 4 do out[#out + 1] = rnd(256) end
        else out[#out + 1] = b end
      end
    else
      out[#out + 1] = rnd(256)
    end
  end
  local s = {}
  for i = 1, #out do s[i] = string.char(out[i]) end
  return table.concat(s)
end

local function run(modname, blocks, blocklen)
  local dis = require(modname)
  local lines, chars, sum = 0, 0, 0
  local mnem = {}
  local sample = {}
  local function out(s)
    lines = lines + 1
    chars = chars + #s
    local m = s:match("^%x+%s+%x+%s+([%a][%w%.]*)")
    if m then mnem[m] = (mnem[m] or 0) + 1 end
    sum = (sum * 31 + #s + (s:byte(-2) or 0)) % 1000000007
    if lines % 4000 == 1 and #sample < 8 then sample[#sample + 1] = s end
  end
  for b = 1, blocks do
    local code = gen_code(blocklen)
    local ctx = dis.create(code, 0x400000 + b * 0x1000, out)
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

if MODE == "32" or MODE == "both" then run("jit.dis_x86", math.max(1, math.floor(10 * N)), 2000) end
if MODE == "64" or MODE == "both" then run("jit.dis_x64", math.max(1, math.floor(10 * N)), 2000) end
