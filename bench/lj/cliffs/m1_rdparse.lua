-- M1 / H1: a recursive-descent evaluator over token arrays (expr -> term -> factor -> expr), called from a hot loop.
-- H1 fix: the old version (m1_rdparse_const.lua) evaluated ONE constant token array, which LuaJIT unrolls and folds to
-- "s = s + 40". Here 64 different pseudo-random expressions (built once, Park-Miller LCG, exact in doubles) are
-- evaluated in turn, so the work depends on the data. Plain 5.1-compatible Lua; same output on PUC/ours/LuaJIT.
--   lua m1_rdparse.lua [OUTER]   (OUTER x 64 evaluations)
local seed = 12345
local function rnd(n) seed = (seed * 16807) % 2147483647 return seed % n + 1 end

local function gen(depth, out)
  local terms = rnd(3) + 1
  for t = 1, terms do
    if t > 1 then out[#out + 1] = (rnd(2) == 1) and "+" or "-" end
    local factors = rnd(3)
    for f = 1, factors do
      if f > 1 then out[#out + 1] = "*" end
      if depth > 0 and rnd(4) == 1 then
        out[#out + 1] = "("
        gen(depth - 1, out)
        out[#out + 1] = ")"
      else
        out[#out + 1] = rnd(9)
      end
    end
  end
end

local E = {}
for j = 1, 64 do
  local toks = {}
  gen(2, toks)
  E[j] = toks
end

local toks, pos
local expr
local function factor()
  local t = toks[pos]
  pos = pos + 1
  if t == "(" then
    local v = expr()
    pos = pos + 1
    return v
  end
  return t
end
local function term()
  local v = factor()
  while toks[pos] == "*" do pos = pos + 1; v = v * factor() end
  return v
end
function expr()
  local v = term()
  while toks[pos] == "+" or toks[pos] == "-" do
    local op = toks[pos]
    pos = pos + 1
    if op == "+" then v = v + term() else v = v - term() end
  end
  return v
end

local outer = tonumber(arg and arg[1]) or 2000
local s, ntok = 0, 0
for j = 1, 64 do ntok = ntok + #E[j] end
for o = 1, outer do
  for j = 1, 64 do
    toks = E[j]
    pos = 1
    s = s + expr()
  end
end
print(s, ntok)
