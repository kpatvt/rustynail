-- Lua-in-Lua: tokenizer + recursive-descent parser + tree-walking evaluator for a tiny language
local kw = { ["while"] = true, ["do"] = true, ["end"] = true, ["if"] = true, ["then"] = true, ["else"] = true, fn = true, ["return"] = true }
local function tokenize(src)
  local toks, pos, n = {}, 1, 0
  while true do
    pos = src:find("%S", pos)
    if not pos then break end
    local c = src:sub(pos, pos)
    local s, e
    if c:match("%d") then
      s, e = src:find("^%d+%.?%d*", pos)
      n = n + 1; toks[n] = { "num", tonumber(src:sub(s, e)) }
    elseif c:match("[%a_]") then
      s, e = src:find("^[%w_]+", pos)
      local w = src:sub(s, e)
      n = n + 1; toks[n] = { kw[w] and w or "id", w }
    else
      s, e = src:find("^[=<>~]=", pos)
      if not s then s, e = pos, pos end
      n = n + 1; toks[n] = { src:sub(s, e), src:sub(s, e) }
    end
    pos = e + 1
  end
  n = n + 1; toks[n] = { "eof" }
  return toks
end

local function parse(toks)
  local p = 1
  local function peek() return toks[p][1] end
  local function nxt() local t = toks[p]; p = p + 1; return t end
  local function expect(k) local t = nxt(); assert(t[1] == k, "expected " .. k .. " got " .. t[1]); return t end
  local expr, block
  local prec = { ["=="] = 1, ["<"] = 1, ["<="] = 1, [">"] = 1, ["+"] = 2, ["-"] = 2, ["*"] = 3, ["/"] = 3, ["%"] = 3 }
  local function primary()
    local t = nxt()
    if t[1] == "num" then return { "num", t[2] }
    elseif t[1] == "id" then
      if peek() == "(" then
        nxt()
        local args = {}
        if peek() ~= ")" then
          repeat args[#args + 1] = expr(0) until not (peek() == "," and nxt())
        end
        expect(")")
        return { "call", t[2], args }
      end
      return { "var", t[2] }
    elseif t[1] == "(" then local e = expr(0); expect(")"); return e
    end
    error("bad token " .. t[1])
  end
  function expr(minp)
    local l = primary()
    while true do
      local op = peek()
      local pr = prec[op]
      if not pr or pr <= minp then return l end
      nxt()
      l = { "bin", op, l, expr(pr) }
    end
  end
  function block()
    local stmts = {}
    while true do
      local k = peek()
      if k == "eof" or k == "end" or k == "else" then return stmts end
      if k == "while" then
        nxt(); local c = expr(0); expect("do"); local b = block(); expect("end")
        stmts[#stmts + 1] = { "while", c, b }
      elseif k == "if" then
        nxt(); local c = expr(0); expect("then"); local b = block(); local e
        if peek() == "else" then nxt(); e = block() end
        expect("end")
        stmts[#stmts + 1] = { "if", c, b, e }
      elseif k == "fn" then
        nxt(); local name = expect("id")[2]; expect("(")
        local ps = {}
        if peek() ~= ")" then repeat ps[#ps + 1] = expect("id")[2] until not (peek() == "," and nxt()) end
        expect(")"); local b = block(); expect("end")
        stmts[#stmts + 1] = { "fn", name, ps, b }
      elseif k == "return" then
        nxt(); stmts[#stmts + 1] = { "return", expr(0) }
      else
        local name = expect("id")[2]; expect("=")
        stmts[#stmts + 1] = { "set", name, expr(0) }
      end
    end
  end
  return block()
end

local ops = {
  ["+"] = function(a, b) return a + b end, ["-"] = function(a, b) return a - b end,
  ["*"] = function(a, b) return a * b end, ["/"] = function(a, b) return math.floor(a / b) end,
  ["%"] = function(a, b) return a % b end,
  ["<"] = function(a, b) return a < b and 1 or 0 end, ["<="] = function(a, b) return a <= b and 1 or 0 end,
  [">"] = function(a, b) return a > b and 1 or 0 end, ["=="] = function(a, b) return a == b and 1 or 0 end,
}

local eval, exec
function eval(e, env)
  local k = e[1]
  if k == "num" then return e[2]
  elseif k == "var" then return env.vars[e[2]]
  elseif k == "bin" then return ops[e[2]](eval(e[3], env), eval(e[4], env))
  else
    local f = env.fns[e[2]]
    local vars = {}
    for i, a in ipairs(e[3]) do vars[f[1][i]] = eval(a, env) end
    return exec(f[2], { vars = vars, fns = env.fns })
  end
end
function exec(stmts, env)
  for i = 1, #stmts do
    local s = stmts[i]
    local k = s[1]
    if k == "set" then env.vars[s[2]] = eval(s[3], env)
    elseif k == "while" then
      while eval(s[2], env) ~= 0 do
        local r = exec(s[3], env)
        if r then return r end
      end
    elseif k == "if" then
      local r
      if eval(s[2], env) ~= 0 then r = exec(s[3], env) elseif s[4] then r = exec(s[4], env) end
      if r then return r end
    elseif k == "fn" then env.fns[s[2]] = { s[3], s[4] }
    elseif k == "return" then return eval(s[2], env)
    end
  end
end

local prog = [[
fn fib(n) if n < 2 then return n end return fib(n - 1) + fib(n - 2) end
fn collatz(n) c = 0 while n > 1 do if n % 2 == 0 then n = n / 2 else n = 3 * n + 1 end c = c + 1 end return c end
fn run(m) i = 1 best = 0 while i <= m do c = collatz(i) if c > best then best = c end i = i + 1 end return best end
a = fib(15)
b = run(300)
return a * 1000 + b
]]
local res
for i = 1, 6 do
  local ast = parse(tokenize(prog))
  res = exec(ast, { vars = {}, fns = {} })
end
print(res)
