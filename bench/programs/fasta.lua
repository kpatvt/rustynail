local n = tonumber(arg and arg[1]) or 10000000
local IM, IA, IC = 139968, 3877, 29573
local last = 42
local function random(max) last = (last * IA + IC) % IM; return max * last / IM end
local alu = "GGCCGGGCGCGGTGGCTCACGCCTGTAATCCCAGCACTTTGGGAGGCCGAGGCGGGCGGATCACCTGAGGTCAGGAGTTCGAGACCAGCCTGGCCAACATGGTGAAACCCCGTCTCTACTAAAAATACAAAAATTAGCCGGG"
local buf, check = {}, 0
local out = {}
for i = 1, n do
  local r = random(1.0)
  local c = r < 0.3 and "a" or r < 0.5 and "c" or r < 0.8 and "g" or "t"
  buf[#buf + 1] = c
  if #buf == 60 then local line = table.concat(buf); check = check + #line + line:byte(1); out[#out + 1] = line; buf = {} end
end
local rep = alu:rep(n // 100):sub(1, n)
print(check, #out, #rep, rep:byte(n))
