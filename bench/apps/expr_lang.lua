-- expr_lang: tokenizer + precedence-climbing parser + AST tree-walking evaluator for a tiny arithmetic language
-- (assignments, + - * / %, comparison, unary minus, parentheses, calls of min/max/abs/clamp and of user
-- functions defined with `fn name(a, b) = expr`).  The program text is generated deterministically, parsed once per
-- round (string.find with patterns for tokens), then evaluated several times with different inputs.
-- Integer-valued arithmetic kept below 2^31 by `% 1000003` in assignments; plain Lua 5.1-compatible idiom.
local ROUNDS = tonumber(arg and arg[1]) or 6
local find, sub, byte = string.find, string.sub, string.byte
local floor = math.floor
local insert = table.insert

-- ---- program generator -------------------------------------------------------------------------------
local seed = 4242
local function rnd(n) seed = (seed * 16807) % 2147483647 return seed % n + 1 end
local function genProgram()
  local lines = {}
  lines[#lines + 1] = "fn mix(a, b) = (a * 31 + b * 17) % 1009"
  lines[#lines + 1] = "fn sq(a) = a * a % 10007"
  lines[#lines + 1] = "fn pick(c, a, b) = c * a + (1 - c) * b"
  local names = {}
  for i = 1, 24 do names[i] = "v" .. i end
  for i = 1, #names do lines[#lines + 1] = names[i] .. " = " .. rnd(100) end
  local function atom(depth)
    local r = rnd(10)
    if r <= 3 then return tostring(rnd(50)) end
    if r <= 6 then return names[rnd(#names)] end
    if depth > 0 and r <= 8 then return "(" .. atom(depth - 1) .. " " .. ({ "+", "-", "*" })[rnd(3)] .. " " .. atom(depth - 1) .. ")" end
    if depth > 0 and r == 9 then return "mix(" .. atom(depth - 1) .. ", " .. atom(depth - 1) .. ")" end
    return "-" .. atom(0)
  end
  for i = 1, 160 do
    local e = atom(3)
    for k = 1, rnd(3) do e = e .. " " .. ({ "+", "-", "*", "%" })[rnd(4)] .. " " .. atom(2) end
    local n = names[rnd(#names)]
    local form = rnd(6)
    if form == 1 then
      lines[#lines + 1] = n .. " = max(" .. e .. ", " .. atom(1) .. ") % 1000003"
    elseif form == 2 then
      lines[#lines + 1] = n .. " = pick(" .. atom(1) .. " < " .. atom(1) .. ", sq(" .. atom(2) .. "), " .. e .. ") % 1000003"
    elseif form == 3 then
      lines[#lines + 1] = n .. " = abs(" .. e .. " - " .. atom(2) .. ") % 1000003"
    else
      lines[#lines + 1] = n .. " = (" .. e .. ") % 1000003"
    end
  end
  return table.concat(lines, "\n")
end

-- ---- tokenizer ------------------------------------------------------------------------------------------
local function tokenize(src)
  local toks = {}
  local pos, len = 1, #src
  while pos <= len do
    local s, e = find(src, "^[ \t\r]+", pos)
    if s then
      pos = e + 1
    elseif find(src, "^\n", pos) then
      insert(toks, { t = "nl" })
      pos = pos + 1
    else
      s, e = find(src, "^%d+", pos)
      if s then
        insert(toks, { t = "num", v = tonumber(sub(src, s, e)) })
        pos = e + 1
      else
        s, e = find(src, "^[%a_][%w_]*", pos)
        if s then
          local w = sub(src, s, e)
          if w == "fn" then insert(toks, { t = "fn" }) else insert(toks, { t = "id", v = w }) end
          pos = e + 1
        else
          local c = sub(src, pos, pos)
          insert(toks, { t = c })
          pos = pos + 1
        end
      end
    end
  end
  insert(toks, { t = "eof" })
  return toks
end

-- ---- parser (precedence climbing) -> AST of tables -------------------------------------------------------
local PREC = { ["<"] = 1, ["+"] = 2, ["-"] = 2, ["*"] = 3, ["/"] = 3, ["%"] = 3 }
local P = {}
P.__index = P
function P:peek() return self.toks[self.i] end
function P:next() local t = self.toks[self.i]; self.i = self.i + 1; return t end
function P:expect(t)
  local x = self:next()
  if x.t ~= t then error("expected " .. t .. " got " .. x.t) end
  return x
end
function P:primary()
  local t = self:next()
  if t.t == "num" then return { k = "num", v = t.v } end
  if t.t == "-" then return { k = "neg", e = self:primary() } end
  if t.t == "(" then
    local e = self:expr(0)
    self:expect(")")
    return e
  end
  if t.t == "id" then
    if self:peek().t == "(" then
      self:next()
      local args = {}
      if self:peek().t ~= ")" then
        insert(args, self:expr(0))
        while self:peek().t == "," do self:next(); insert(args, self:expr(0)) end
      end
      self:expect(")")
      return { k = "call", name = t.v, args = args }
    end
    return { k = "var", name = t.v }
  end
  error("unexpected token " .. t.t)
end
function P:expr(minPrec)
  local lhs = self:primary()
  while true do
    local op = self:peek().t
    local prec = PREC[op]
    if prec == nil or prec < minPrec then break end
    self:next()
    local rhs = self:expr(prec + 1)
    lhs = { k = "bin", op = op, l = lhs, r = rhs }
  end
  return lhs
end
local function parse(src)
  local p = setmetatable({ toks = tokenize(src), i = 1 }, P)
  local prog = { fns = {}, stmts = {} }
  while p:peek().t ~= "eof" do
    if p:peek().t == "nl" then
      p:next()
    elseif p:peek().t == "fn" then
      p:next()
      local name = p:expect("id").v
      p:expect("(")
      local params = {}
      if p:peek().t ~= ")" then
        insert(params, p:expect("id").v)
        while p:peek().t == "," do p:next(); insert(params, p:expect("id").v) end
      end
      p:expect(")")
      p:expect("=")
      prog.fns[name] = { params = params, body = p:expr(0) }
    else
      local name = p:expect("id").v
      p:expect("=")
      insert(prog.stmts, { name = name, e = p:expr(0) })
    end
  end
  return prog
end

-- ---- evaluator --------------------------------------------------------------------------------------------
local eval
local builtin = {
  max = function(a, b) if a > b then return a end return b end,
  min = function(a, b) if a < b then return a end return b end,
  abs = function(a) if a < 0 then return -a end return a end,
}
local function evalBin(op, a, b)
  if op == "+" then return a + b
  elseif op == "-" then return a - b
  elseif op == "*" then return (a * b) % 1000003  -- keep every product below 2^53 (exact in doubles)
  elseif op == "<" then if a < b then return 1 end return 0
  elseif op == "%" then if b == 0 then return 0 end return a % b
  else if b == 0 then return 0 end return floor(a / b) end
end
function eval(n, env, prog)
  local k = n.k
  if k == "num" then return n.v
  elseif k == "var" then return env[n.name] or 0
  elseif k == "bin" then return evalBin(n.op, eval(n.l, env, prog), eval(n.r, env, prog))
  elseif k == "neg" then return -eval(n.e, env, prog)
  else
    local args = n.args
    local vals = {}
    for i = 1, #args do vals[i] = eval(args[i], env, prog) end
    local b = builtin[n.name]
    if b then return b(vals[1], vals[2]) end
    local f = prog.fns[n.name]
    local fenv = {}
    for i = 1, #f.params do fenv[f.params[i]] = vals[i] end
    return eval(f.body, fenv, prog)
  end
end
local function run(prog, input)
  local env = { v1 = input }
  for i = 1, #prog.stmts do
    local s = prog.stmts[i]
    env[s.name] = eval(s.e, env, prog) % 1000003
  end
  local sum = 0
  for i = 1, 24 do sum = (sum + (env["v" .. i] or 0) * i) % 1000003 end
  return sum
end

local src = genProgram()
local check, stmts = 0, 0
for r = 1, ROUNDS do
  local prog = parse(src)
  stmts = #prog.stmts
  for input = 1, 40 do check = (check + run(prog, input * r)) % 1000003 end
end
print("expr_lang", #src, stmts, check)
