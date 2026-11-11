-- A closure-based parser combinator library (lpeg-free, in the style of lua-parser-combinators / "pc.lua"),
-- used to parse and evaluate arithmetic expressions with variables and a small JSON-like data language.
local P = {}

-- a parser is function(input, pos) -> ok, value, newpos
function P.lit(s)
  local n = #s
  return function(inp, pos)
    if inp:sub(pos, pos + n - 1) == s then return true, s, pos + n end
    return false, nil, pos
  end
end

function P.pat(p)
  local anchored = "^(" .. p .. ")"
  return function(inp, pos)
    local m = inp:match(anchored, pos)
    if m then return true, m, pos + #m end
    return false, nil, pos
  end
end

function P.seq(...)
  local ps = { ... }
  local n = select("#", ...)
  return function(inp, pos)
    local vals = {}
    local p = pos
    for i = 1, n do
      local ok, v, np = ps[i](inp, p)
      if not ok then return false, nil, pos end
      vals[i] = v
      p = np
    end
    return true, vals, p
  end
end

function P.alt(...)
  local ps = { ... }
  return function(inp, pos)
    for i = 1, #ps do
      local ok, v, np = ps[i](inp, pos)
      if ok then return true, v, np end
    end
    return false, nil, pos
  end
end

function P.many(p)
  return function(inp, pos)
    local vals, n = {}, 0
    while true do
      local ok, v, np = p(inp, pos)
      if not ok or np == pos then break end
      n = n + 1
      vals[n] = v
      pos = np
    end
    return true, vals, pos
  end
end

function P.map(p, f)
  return function(inp, pos)
    local ok, v, np = p(inp, pos)
    if ok then return true, f(v), np end
    return false, nil, pos
  end
end

function P.opt(p, default)
  return function(inp, pos)
    local ok, v, np = p(inp, pos)
    if ok then return true, v, np end
    return true, default, pos
  end
end

function P.lazy(thunk)
  local p
  return function(inp, pos)
    p = p or thunk()
    return p(inp, pos)
  end
end

local ws = P.pat("%s*")
function P.tok(p)
  return P.map(P.seq(ws, p, ws), function(v) return v[2] end)
end

function P.sepBy(p, sep)
  local rest = P.many(P.map(P.seq(sep, p), function(v) return v[2] end))
  return P.opt(P.map(P.seq(p, rest), function(v)
    local out = { v[1] }
    for i = 1, #v[2] do out[#out + 1] = v[2][i] end
    return out
  end), {})
end

-- chainl1: left-assoc binary operators
function P.chainl1(p, op)
  return function(inp, pos)
    local ok, acc, np = p(inp, pos)
    if not ok then return false, nil, pos end
    while true do
      local ok2, f, np2 = op(inp, np)
      if not ok2 then break end
      local ok3, rhs, np3 = p(inp, np2)
      if not ok3 then break end
      acc = f(acc, rhs)
      np = np3
    end
    return true, acc, np
  end
end

-- Expression grammar -> AST
local number = P.tok(P.map(P.pat("%d+%.?%d*"), function(s) return { "num", tonumber(s) } end))
local ident = P.tok(P.map(P.pat("[%a_][%w_]*"), function(s) return { "var", s } end))
local function binop(sym, tag)
  return P.map(P.tok(P.lit(sym)), function() return function(a, b) return { tag, a, b } end end)
end
local expr
local call = P.map(P.seq(ident, P.tok(P.lit("(")), P.sepBy(P.lazy(function() return expr end), P.tok(P.lit(","))), P.tok(P.lit(")"))),
  function(v) return { "call", v[1][2], v[3] } end)
local paren = P.map(P.seq(P.tok(P.lit("(")), P.lazy(function() return expr end), P.tok(P.lit(")"))), function(v) return v[2] end)
local neg = P.map(P.seq(P.tok(P.lit("-")), P.lazy(function() return expr end)), function(v) return { "neg", v[2] } end)
local atom = P.alt(number, call, ident, paren, neg)
local power = P.chainl1(atom, binop("^", "pow"))
local term = P.chainl1(power, P.alt(binop("*", "mul"), binop("/", "div"), binop("%", "mod")))
expr = P.chainl1(term, P.alt(binop("+", "add"), binop("-", "sub")))

local funcs = {
  max = math.max, min = math.min, abs = math.abs,
  sq = function(x) return x * x end,
}

local function eval(node, env)
  local t = node[1]
  if t == "num" then return node[2]
  elseif t == "var" then return env[node[2]] or 0
  elseif t == "add" then return eval(node[2], env) + eval(node[3], env)
  elseif t == "sub" then return eval(node[2], env) - eval(node[3], env)
  elseif t == "mul" then return eval(node[2], env) * eval(node[3], env)
  elseif t == "div" then return eval(node[2], env) / eval(node[3], env)
  elseif t == "mod" then return eval(node[2], env) % eval(node[3], env)
  elseif t == "pow" then return eval(node[2], env) ^ eval(node[3], env)
  elseif t == "neg" then return -eval(node[2], env)
  elseif t == "call" then
    local args = {}
    for i, a in ipairs(node[3]) do args[i] = eval(a, env) end
    return funcs[node[2]](unpack(args))
  end
  error("bad node " .. tostring(t))
end
unpack = unpack or table.unpack

-- small data language: { key = value, list = [1, 2, "x"] }
local value
local str = P.tok(P.map(P.pat('"[^"]*"'), function(s) return s:sub(2, -2) end))
local num = P.tok(P.map(P.pat("%-?%d+"), tonumber))
local bool = P.tok(P.alt(P.map(P.lit("true"), function() return true end), P.map(P.lit("false"), function() return false end)))
local list = P.map(P.seq(P.tok(P.lit("[")), P.sepBy(P.lazy(function() return value end), P.tok(P.lit(","))), P.tok(P.lit("]"))),
  function(v) return v[2] end)
local pair = P.map(P.seq(P.tok(P.pat("[%a_][%w_]*")), P.tok(P.lit("=")), P.lazy(function() return value end)),
  function(v) return { v[1], v[3] } end)
local obj = P.map(P.seq(P.tok(P.lit("{")), P.sepBy(pair, P.tok(P.lit(","))), P.tok(P.lit("}"))), function(v)
  local o = {}
  for _, kv in ipairs(v[2]) do o[kv[1]] = kv[2] end
  return o
end)
value = P.alt(str, num, bool, list, obj)

local N = tonumber(arg and arg[1]) or 3000
local exprs = {
  "1 + 2 * 3 - 4 / 2",
  "max(a, b) * (c + 3) - sq(d) % 7",
  "-(x + y) * (x - y) + abs(z - 100)",
  "((((a + 1) * 2) + 3) * 4) / (b + 1)",
  "min(1, 2, 3, a, b) + 2 ^ 3 + sq(sq(2))",
  "a * b * c * d + a + b + c + d - x * y",
}
local data_src = '{ name = "widget", tags = ["a", "b", "c"], size = { w = 10, h = -20 }, ok = true, list = [1, 2, 3, [4, 5, [6]], { k = "v" }] }'

local total = 0
local cnt = 0
for r = 1, N do
  for i, src in ipairs(exprs) do
    local ok, ast, pos = expr(src, 1)
    assert(ok and pos == #src + 1, src)
    for k = 1, 3 do
      local env = { a = r % 10, b = k + 1, c = i, d = r % 5, x = k * 1.5, y = 2, z = r }
      total = total + eval(ast, env)
    end
    cnt = cnt + 1
  end
  local ok, d = value(data_src, 1)
  assert(ok)
  total = total + d.size.w + d.size.h + #d.tags + #d.list[4][3]
end
print("parsed", cnt, string.format("total %.6f", total))
