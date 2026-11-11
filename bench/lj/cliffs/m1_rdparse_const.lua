-- INVALID as a LuaJIT comparison (H1): constant token array + pos=1 at every start lets LuaJIT unroll and fold the whole evaluator (marginal ~3 cycles per evaluation, bimodal warm-up); kept as the M1 repro for our own engine. See m1_rdparse.lua for the varying-input version.
-- M1: a recursive-descent evaluator over a token array (expr -> term -> factor -> expr), called from a hot loop.
--   lua m1_rdparse.lua   (N iterations)
local toks = { 3, "+", 4, "*", "(", 2, "+", 5, ")", "-", 6, "*", 2, "+", 7, "*", 3 }
local pos
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
local n = tonumber(arg and arg[1]) or 150000
local s = 0
for i = 1, n do
  pos = 1
  s = s + expr()
end
print(s)
