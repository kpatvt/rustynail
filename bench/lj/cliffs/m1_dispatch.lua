-- M1: a table-driven interpreter loop: a table of handler functions, an AST walked by mutually recursive functions
-- (eval / run), nodes with string-keyed fields, `return f(...)` (tail call) in functions with a generic `for`.
--   lua m1_dispatch.lua   (N iterations)
local eval, run
local H = {}
H.num = function(n, env) return n.v end
H.var = function(n, env) return env[n.name] end
H.add = function(n, env) return eval(n.a, env) + eval(n.b, env) end
H.mul = function(n, env) return eval(n.a, env) * eval(n.b, env) end
H.seq = function(n, env)
  local r
  for _, s in ipairs(n.list) do r = run(s, env) end
  return r
end
H.set = function(n, env) env[n.name] = eval(n.e, env); return env[n.name] end
function eval(n, env) return H[n.k](n, env) end
function run(n, env) return eval(n, env) end
local function N(k, t) t.k = k; return t end
local prog = N("seq", { list = {
  N("set", { name = "x", e = N("add", { a = N("var", { name = "x" }), b = N("num", { v = 3 }) }) }),
  N("set", { name = "y", e = N("mul", { a = N("var", { name = "x" }), b = N("num", { v = 2 }) }) }),
  N("add", { a = N("var", { name = "y" }), b = N("var", { name = "x" }) }),
} })
local n = tonumber(arg and arg[1]) or 150000
local env = { x = 0, y = 0 }
local s = 0
for i = 1, n do
  env.x = i % 100
  s = s + run(prog, env)
end
print(s)
