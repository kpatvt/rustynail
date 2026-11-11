-- Tokenizer + Pratt parser for arithmetic/boolean expressions; AST compiled to Lua closures
-- and to a stack bytecode with an interpreter loop. small: N=2
local N = tonumber(arg and arg[1]) or 14000

-- Lexer ---------------------------------------------------------------------------------
local function tokenize(src)
  local tokens, i = {}, 1
  while i <= #src do
    local c = src:sub(i, i)
    if c:match("%s") then
      i = i + 1
    elseif c:match("%d") then
      local num = src:match("^%d+%.?%d*", i)
      tokens[#tokens + 1] = { type = "num", value = tonumber(num) }
      i = i + #num
    elseif c:match("[%a_]") then
      local id = src:match("^[%w_]+", i)
      tokens[#tokens + 1] = { type = "id", value = id }
      i = i + #id
    else
      local two = src:sub(i, i + 1)
      if two == "==" or two == "!=" or two == "<=" or two == ">=" or two == "&&" or two == "||" then
        tokens[#tokens + 1] = { type = "op", value = two }
        i = i + 2
      else
        tokens[#tokens + 1] = { type = "op", value = c }
        i = i + 1
      end
    end
  end
  tokens[#tokens + 1] = { type = "eof" }
  return tokens
end

-- Pratt parser --------------------------------------------------------------------------
local binary = {
  ["||"] = { 1, false }, ["&&"] = { 2, false },
  ["=="] = { 3, false }, ["!="] = { 3, false },
  ["<"] = { 4, false }, [">"] = { 4, false }, ["<="] = { 4, false }, [">="] = { 4, false },
  ["+"] = { 5, false }, ["-"] = { 5, false },
  ["*"] = { 6, false }, ["/"] = { 6, false }, ["%"] = { 6, false },
  ["^"] = { 8, true },
}

local function parse(src)
  local toks, pos = tokenize(src), 1
  local function peek() return toks[pos] end
  local function advance() pos = pos + 1; return toks[pos - 1] end
  local function expect(v)
    local t = advance()
    if t.value ~= v then error("expected " .. v .. " got " .. tostring(t.value)) end
  end

  local expr
  local function primary()
    local t = advance()
    if t.type == "num" then return { k = "num", v = t.value } end
    if t.type == "id" then
      if peek().value == "(" then
        advance()
        local args = {}
        if peek().value ~= ")" then
          repeat
            args[#args + 1] = expr(0)
            local more = peek().value == ","
            if more then advance() end
          until not more
        end
        expect(")")
        return { k = "call", name = t.value, args = args }
      end
      return { k = "var", name = t.value }
    end
    if t.value == "(" then
      local e = expr(0)
      expect(")")
      return e
    end
    if t.value == "-" then return { k = "neg", expr(7) } end
    if t.value == "!" then return { k = "not", expr(7) } end
    error("unexpected token " .. tostring(t.value))
  end

  function expr(minPrec)
    local left = primary()
    while true do
      local t = peek()
      local info = t.type == "op" and binary[t.value]
      if not info or info[1] < minPrec then break end
      advance()
      local right = expr(info[2] and info[1] or info[1] + 1)
      left = { k = "bin", op = t.value, left, right }
    end
    return left
  end

  local ast = expr(0)
  if peek().type ~= "eof" then error("trailing tokens") end
  return ast
end

-- Runtime functions -------------------------------------------------------------------------
local funcs = {
  max = math.max, min = math.min, abs = math.abs,
  sq = function(x) return x * x end,
  clamp = function(x, lo, hi) return x < lo and lo or (x > hi and hi or x) end,
}

local arith = {
  ["+"] = function(a, b) return a + b end,
  ["-"] = function(a, b) return a - b end,
  ["*"] = function(a, b) return a * b end,
  ["/"] = function(a, b) if b == 0 then return 0 end return a / b end,
  ["%"] = function(a, b) if b == 0 then return 0 end return a % b end,
  ["^"] = function(a, b) return a ^ b end,
  ["<"] = function(a, b) return a < b and 1 or 0 end,
  [">"] = function(a, b) return a > b and 1 or 0 end,
  ["<="] = function(a, b) return a <= b and 1 or 0 end,
  [">="] = function(a, b) return a >= b and 1 or 0 end,
  ["=="] = function(a, b) return a == b and 1 or 0 end,
  ["!="] = function(a, b) return a ~= b and 1 or 0 end,
  ["&&"] = function(a, b) return (a ~= 0 and b ~= 0) and 1 or 0 end,
  ["||"] = function(a, b) return (a ~= 0 or b ~= 0) and 1 or 0 end,
}

-- Closure compiler ----------------------------------------------------------------------------
local function toClosure(node)
  local k = node.k
  if k == "num" then
    local v = node.v
    return function() return v end
  elseif k == "var" then
    local name = node.name
    return function(env) return env[name] or 0 end
  elseif k == "neg" then
    local e = toClosure(node[1])
    return function(env) return -e(env) end
  elseif k == "not" then
    local e = toClosure(node[1])
    return function(env) return e(env) == 0 and 1 or 0 end
  elseif k == "bin" then
    local l, r, f = toClosure(node[1]), toClosure(node[2]), arith[node.op]
    return function(env) return f(l(env), r(env)) end
  elseif k == "call" then
    local fn = funcs[node.name] or error("no function " .. node.name)
    local args = {}
    for i, a in ipairs(node.args) do args[i] = toClosure(a) end
    return function(env)
      local vals = {}
      for i = 1, #args do vals[i] = args[i](env) end
      return fn((table.unpack or unpack)(vals))
    end
  end
  error("bad node " .. tostring(k))
end

-- Bytecode compiler + VM ------------------------------------------------------------------------
local function emit(node, code)
  local k = node.k
  if k == "num" then code[#code + 1] = { "push", node.v }
  elseif k == "var" then code[#code + 1] = { "load", node.name }
  elseif k == "neg" then emit(node[1], code); code[#code + 1] = { "neg" }
  elseif k == "not" then emit(node[1], code); code[#code + 1] = { "not" }
  elseif k == "bin" then
    emit(node[1], code); emit(node[2], code)
    code[#code + 1] = { "op", arith[node.op] }
  elseif k == "call" then
    for _, a in ipairs(node.args) do emit(a, code) end
    code[#code + 1] = { "call", funcs[node.name], #node.args }
  end
  return code
end

local function run(code, env)
  local stack, sp = {}, 0
  for pc = 1, #code do
    local ins = code[pc]
    local op = ins[1]
    if op == "push" then sp = sp + 1; stack[sp] = ins[2]
    elseif op == "load" then sp = sp + 1; stack[sp] = env[ins[2]] or 0
    elseif op == "neg" then stack[sp] = -stack[sp]
    elseif op == "not" then stack[sp] = stack[sp] == 0 and 1 or 0
    elseif op == "op" then
      local b = stack[sp]; sp = sp - 1
      stack[sp] = ins[2](stack[sp], b)
    elseif op == "call" then
      local n = ins[3]
      local args = {}
      for i = n, 1, -1 do args[i] = stack[sp]; sp = sp - 1 end
      sp = sp + 1
      stack[sp] = ins[2]((table.unpack or unpack)(args))
    end
  end
  return stack[sp]
end

-- Pretty printing -----------------------------------------------------------------------------------
local function show(node)
  local k = node.k
  if k == "num" then return string.format("%g", node.v)
  elseif k == "var" then return node.name
  elseif k == "neg" then return "(-" .. show(node[1]) .. ")"
  elseif k == "not" then return "(!" .. show(node[1]) .. ")"
  elseif k == "bin" then return "(" .. show(node[1]) .. " " .. node.op .. " " .. show(node[2]) .. ")"
  else
    local parts = {}
    for i, a in ipairs(node.args) do parts[i] = show(a) end
    return node.name .. "(" .. table.concat(parts, ", ") .. ")"
  end
end

-- Driver ---------------------------------------------------------------------------------------------
local sources = {
  "1 + 2 * 3 - 4 / 2",
  "x * x + y * y < 100 && !(x == y)",
  "max(x, y) - min(x, y) + abs(x - 2 * y)",
  "2 ^ 3 ^ 2 + sq(z) % 7",
  "clamp(x * 3 - y, 0, 50) + (z > 3 || x <= 2) * 10",
  "-x + -(-y) * 2 - z / (x - x)",
  "(x + 1) * (y + 2) * (z + 3) / 5 + max(1, 2, 3)",
  "x >= y && y >= z || z != 4",
}

local envs = {}
for i = 1, N do
  envs[i] = { x = i % 17, y = (i * 7) % 13, z = (i * 3) % 11 }
end

local asts, closures, codes = {}, {}, {}
for i, s in ipairs(sources) do
  asts[i] = parse(s)
  closures[i] = toClosure(asts[i])
  codes[i] = emit(asts[i], {})
end

local mismatches = 0
local sums1, sums2 = {}, {}
for i = 1, #sources do
  local s1, s2 = 0, 0
  for _, env in ipairs(envs) do
    local a = closures[i](env)
    local b = run(codes[i], env)
    if a ~= b then mismatches = mismatches + 1 end
    s1 = s1 + a
    s2 = s2 + b
  end
  sums1[i], sums2[i] = s1, s2
  print(string.format("%-52s %14.3f  %2d ops", show(asts[i]):sub(1, 52), s1, #codes[i]))
end

-- Parse throughput: generate bigger expressions and parse them
local parsed = 0
for i = 1, N do
  local src = sources[i % #sources + 1] .. " + " .. sources[(i * 3) % #sources + 1]
  local ast = parse(src)
  if toClosure(ast)(envs[i]) == run(emit(ast, {}), envs[i]) then parsed = parsed + 1 end
end
print(string.format("mismatches=%d reparsed ok=%d", mismatches, parsed))
local _, err = pcall(parse, "1 + * 2")
print((tostring(err):gsub("^.-:%d+: ", "")))
