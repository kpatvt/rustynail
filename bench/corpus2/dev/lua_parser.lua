-- Recursive-descent parser for a Lua subset (locals, functions, if/while/for, tables, expressions)
-- producing an AST, then a pretty-printer and a constant folder. small: N=1
local N = tonumber(arg and arg[1]) or 1500

-- Lexer ---------------------------------------------------------------------------------------
local keywords = {}
for w in ("local function if then else elseif end while do for in return and or not true false nil"):gmatch("%a+") do
  keywords[w] = true
end
local function lex(src)
  local toks, pos, line = {}, 1, 1
  local function push(t, v, len) toks[#toks + 1] = { t = t, v = v, line = line }; pos = pos + len end
  while pos <= #src do
    local c = src:sub(pos, pos)
    local two = src:sub(pos, pos + 1)
    if c == "\n" then line = line + 1; pos = pos + 1
    elseif c:find("%s") then pos = pos + 1
    elseif two == "--" then pos = src:find("\n", pos, true) or #src + 1
    elseif c:find("%d") then local s = src:match("^%d+%.?%d*", pos); push("num", tonumber(s), #s)
    elseif c:find("[%a_]") then local s = src:match("^[%w_]+", pos); push(keywords[s] and s or "name", s, #s)
    elseif c == '"' then local e = src:find('"', pos + 1, true); push("str", src:sub(pos + 1, e - 1), e - pos + 1)
    elseif two == "==" or two == "~=" or two == "<=" or two == ">=" or two == ".." then push(two, two, 2)
    else push(c, c, 1) end
  end
  push("eof", "<eof>", 0)
  return toks
end

-- Parser ------------------------------------------------------------------------------------------
local Parser = {}
Parser.__index = Parser
function Parser.new(src) return setmetatable({ toks = lex(src), i = 1 }, Parser) end
function Parser:peek() return self.toks[self.i].t end
function Parser:take() local t = self.toks[self.i]; self.i = self.i + 1; return t end
function Parser:accept(t) if self:peek() == t then return self:take() end end
function Parser:expect(t)
  local tok = self:take()
  if tok.t ~= t then error(string.format("line %d: expected '%s' near '%s'", tok.line, t, tostring(tok.v)), 0) end
  return tok
end
function Parser:names()
  local list = { self:expect("name").v }
  while self:accept(",") do list[#list + 1] = self:expect("name").v end
  return list
end
function Parser:exprs()
  local list = { self:expr() }
  while self:accept(",") do list[#list + 1] = self:expr() end
  return list
end
function Parser:cond(kw) local e = self:expr(); self:expect(kw); return e end
function Parser:body() local b = self:block(); self:expect("end"); return b end

local binprec = {
  ["or"] = { 1, 1 }, ["and"] = { 2, 2 },
  ["<"] = { 3, 3 }, [">"] = { 3, 3 }, ["<="] = { 3, 3 }, [">="] = { 3, 3 }, ["~="] = { 3, 3 }, ["=="] = { 3, 3 },
  [".."] = { 5, 4 }, ["+"] = { 6, 6 }, ["-"] = { 6, 6 }, ["*"] = { 7, 7 }, ["/"] = { 7, 7 }, ["%"] = { 7, 7 },
}
local blockEnd = { ["end"] = true, ["else"] = true, ["elseif"] = true, eof = true }
function Parser:block()
  local stmts = {}
  while not blockEnd[self:peek()] do
    if self:accept("return") then
      stmts[#stmts + 1] = { k = "return", (not blockEnd[self:peek()]) and self:expr() or nil }
      break
    end
    stmts[#stmts + 1] = self:statement()
  end
  return stmts
end
function Parser:statement()
  if self:accept("local") then
    if self:accept("function") then
      local name = self:expect("name").v
      return { k = "funcstat", name = name, islocal = true, self:funcbody() }
    end
    local names = self:names()
    return { k = "local", names = names, values = self:accept("=") and self:exprs() or {} }
  elseif self:accept("if") then
    local node = { k = "if", clauses = {} }
    repeat
      local c = self:cond("then")
      node.clauses[#node.clauses + 1] = { cond = c, body = self:block() }
    until not self:accept("elseif")
    if self:accept("else") then node.orelse = self:block() end
    self:expect("end")
    return node
  elseif self:accept("while") then
    local cond = self:cond("do")
    return { k = "while", cond = cond, body = self:body() }
  elseif self:accept("for") then
    local var = self:expect("name").v
    self:expect("=")
    local first = self:expr()
    self:expect(",")
    local range = self:exprs()
    self:expect("do")
    return { k = "fornum", var = var, first, range[1], step = range[2], body = self:body() }
  elseif self:accept("function") then
    local name = self:expect("name").v
    return { k = "funcstat", name = name, self:funcbody() }
  end
  local e = self:suffixed()
  if self:accept("=") then return { k = "assign", target = e, value = self:expr() } end
  return { k = "exprstat", e }
end
function Parser:funcbody()
  self:expect("(")
  local params = self:peek() == "name" and self:names() or {}
  self:expect(")")
  return { k = "function", params = params, body = self:body() }
end
function Parser:primary()
  local tok = self:take()
  local t = tok.t
  if t == "num" or t == "str" or t == "name" then return { k = t, v = tok.v }
  elseif t == "nil" or t == "true" or t == "false" then return { k = t }
  elseif t == "function" then return self:funcbody()
  elseif t == "(" then
    local e = self:expr()
    self:expect(")")
    return { k = "paren", e }
  elseif t == "{" then
    local fields = {}
    while self:peek() ~= "}" do
      local key, keyexpr
      if self:accept("[") then
        keyexpr = self:cond("]")
        self:expect("=")
      elseif self:peek() == "name" and self.toks[self.i + 1].t == "=" then
        key = self:take().v
        self:take()
      end
      fields[#fields + 1] = { key = key, keyexpr = keyexpr, value = self:expr() }
      if not (self:accept(",") or self:accept(";")) then break end
    end
    self:expect("}")
    return { k = "table", fields = fields }
  end
  error(string.format("line %d: unexpected '%s'", tok.line, tostring(tok.v)), 0)
end
function Parser:suffixed()
  local e = self:primary()
  while true do
    if self:accept(".") then e = { k = "index", e, { k = "str", v = self:expect("name").v } }
    elseif self:accept("[") then e = { k = "index", e, self:cond("]") }
    elseif self:accept("(") then
      local args = self:peek() ~= ")" and self:exprs() or {}
      self:expect(")")
      e = { k = "call", fn = e, args = args }
    else return e end
  end
end
function Parser:expr(limit)
  limit = limit or 0
  local left
  local t = self:peek()
  if t == "-" or t == "not" or t == "#" then
    self:take()
    left = { k = "unop", op = t, self:expr(8) }
  else
    left = self:suffixed()
  end
  while true do
    local op = self:peek()
    local prec = binprec[op]
    if not prec or prec[1] <= limit then return left end
    self:take()
    left = { k = "binop", op = op, left, self:expr(prec[2]) }
  end
end
local function parse(src)
  local p = Parser.new(src)
  local chunk = { k = "chunk", body = p:block() }
  p:expect("eof")
  return chunk
end

-- Constant folder (generic bottom-up walk over the AST) ------------------------------------
local folds = 0
local arith = { ["+"] = function(a, b) return a + b end, ["-"] = function(a, b) return a - b end,
  ["*"] = function(a, b) return a * b end }
local function fold(n)
  for key, child in pairs(n) do
    if type(child) == "table" then n[key] = fold(child) end
  end
  if n.k == "binop" then
    if n[1].k == "num" and n[2].k == "num" and arith[n.op] then
      folds = folds + 1
      return { k = "num", v = arith[n.op](n[1].v, n[2].v) }
    elseif n.op == ".." and n[1].k == "str" and n[2].k == "str" then
      folds = folds + 1
      return { k = "str", v = n[1].v .. n[2].v }
    end
  elseif n.k == "unop" and n.op == "-" and n[1].k == "num" then
    folds = folds + 1
    return { k = "num", v = -n[1].v }
  elseif n.k == "paren" and (n[1].k == "num" or n[1].k == "str") then
    return n[1]
  end
  return n
end

-- Pretty printer -----------------------------------------------------------------------------
local show
local function list(items, ind)
  local a = {}
  for i, x in ipairs(items) do a[i] = show(x, ind) end
  return table.concat(a, ", ")
end
local function lines(head, body, ind)
  local out = { head }
  for _, s in ipairs(body) do out[#out + 1] = string.rep("  ", ind + 1) .. show(s, ind + 1) end
  return table.concat(out, "\n")
end
local function block(head, body, ind) return lines(head, body, ind) .. "\n" .. string.rep("  ", ind) .. "end" end
function show(n, ind)
  local k = n.k
  if k == "num" then return string.format("%g", n.v)
  elseif k == "str" then return string.format("%q", n.v)
  elseif k == "name" then return n.v
  elseif k == "nil" or k == "true" or k == "false" then return k
  elseif k == "paren" then return "(" .. show(n[1], ind) .. ")"
  elseif k == "unop" then return n.op .. (n.op == "not" and " " or "") .. show(n[1], ind)
  elseif k == "binop" then return show(n[1], ind) .. " " .. n.op .. " " .. show(n[2], ind)
  elseif k == "index" then return show(n[1], ind) .. "[" .. show(n[2], ind) .. "]"
  elseif k == "call" then return show(n.fn, ind) .. "(" .. list(n.args, ind) .. ")"
  elseif k == "table" then
    local a = {}
    for i, f in ipairs(n.fields) do
      a[i] = (f.key and (f.key .. " = ") or (f.keyexpr and ("[" .. show(f.keyexpr, ind) .. "] = ")) or "") .. show(f.value, ind)
    end
    return "{" .. table.concat(a, ", ") .. "}"
  elseif k == "function" then return block("function(" .. table.concat(n.params, ", ") .. ")", n.body, ind)
  elseif k == "local" then return "local " .. table.concat(n.names, ", ") .. (#n.values > 0 and " = " .. list(n.values, ind) or "")
  elseif k == "funcstat" then return (n.islocal and "local " or "") .. "function " .. n.name .. show(n[1], ind):sub(9)
  elseif k == "assign" then return show(n.target, ind) .. " = " .. show(n.value, ind)
  elseif k == "exprstat" then return show(n[1], ind)
  elseif k == "return" then return "return" .. (n[1] and (" " .. show(n[1], ind)) or "")
  elseif k == "while" then return block("while " .. show(n.cond, ind) .. " do", n.body, ind)
  elseif k == "fornum" then
    return block("for " .. n.var .. " = " .. list(n, ind) .. (n.step and (", " .. show(n.step, ind)) or "") .. " do", n.body, ind)
  elseif k == "if" then
    local out, pad = {}, string.rep("  ", ind)
    for i, c in ipairs(n.clauses) do
      out[i] = (i == 1 and "" or pad) .. lines((i == 1 and "if " or "elseif ") .. show(c.cond, ind) .. " then", c.body, ind)
    end
    if n.orelse then out[#out + 1] = pad .. lines("else", n.orelse, ind) end
    return table.concat(out, "\n") .. "\n" .. pad .. "end"
  end
  error("cannot show " .. tostring(k))
end
local function pretty(ast)
  local out = {}
  for i, s in ipairs(ast.body) do out[i] = show(s, 0) end
  return table.concat(out, "\n")
end
local function countNodes(n)
  local c = n.k and 1 or 0
  for _, v in pairs(n) do
    if type(v) == "table" then c = c + countNodes(v) end
  end
  return c
end

-- Source generation and driver ---------------------------------------------------------------------------------
local template = [[
local limit = %d * 2 + (3 - 1)
local names = { "a" .. "b", x = 1 + 2 * 3, [limit] = -(4 + 5) }
local function fact(n)
  if n <= 1 then return 1 elseif n == 2 then return 2 else return n * fact(n - 1) end
end
local total = 0
for i = 1, limit, 2 do
  total = total + fact(i %% 7) * (2 + 3)
  if total > 1000 and not names.x then total = total - 100 else total = total + 1 end
end
while total > %d do total = total - (10 * 10) end
function report(t, extra) return t.x .. "=" .. total end
local s = report(names, #names, nil, true)
]]

local nodes, sizes, sample = 0, 0, nil
for i = 1, N do
  local src = string.format(template, i, i, 50 + i)
  local ast = parse(src)
  nodes = nodes + countNodes(ast)
  local text = pretty(fold(ast))
  sizes = sizes + #text
  sample = sample or text
  local again = pretty(parse(text))
  if again ~= text then error("pretty printer is not stable on sample " .. i) end
end
print(sample)
print(string.format("samples=%d nodes=%d folds=%d pretty bytes=%d", N, nodes, folds, sizes))

for _, bad in ipairs({ "local = 3", "if x then", "x = (1 + ", "for i = 1 do end" }) do
  print(string.format("%-18s -> %s", bad, tostring(select(2, pcall(parse, bad)))))
end
