//! K4 programs: a recursive tree evaluator whose `call` nodes loop over their arguments (the shape of `apps/expr_lang`'s `eval`): the loop trace of the
//! evaluator inlines the recursion, and the side recordings at exits inside the inlined frames end in the loop of the next level (`InnerLoop`). The
//! `innerside` guard stops those recordings once a family has had `innerside` of them; the result of the run must not depend on it.

use super::progs::Prog;

pub const K4_PROGS: &[Prog] = &[Prog {
    name: "k4_eval_loop",
    small: false,
    src: r##"
local seed = 12345
local function rnd(n) seed = (seed * 1103515245 + 12345) % 2147483648; return seed % n + 1 end
local names = { "a", "b", "c", "d" }
local function gen(depth)
  local r = rnd(10)
  if depth <= 0 or r <= 3 then
    if rnd(2) == 1 then return { k = "num", v = rnd(50) } end
    return { k = "var", name = names[rnd(#names)] }
  elseif r <= 6 then
    local ops = { "+", "-", "*" }
    return { k = "bin", op = ops[rnd(3)], l = gen(depth - 1), r = gen(depth - 1) }
  elseif r == 7 then
    return { k = "neg", e = gen(depth - 1) }
  else
    local n = rnd(3)
    local args = {}
    for i = 1, n + 1 do args[i] = gen(depth - 1) end
    return { k = "call", name = (rnd(2) == 1) and "max" or "min", args = args }
  end
end
local floor = math.floor
local builtin = {
  max = function(a, b) if a > b then return a end return b end,
  min = function(a, b) if a < b then return a end return b end,
}
local function evalBin(op, a, b)
  if op == "+" then return a + b
  elseif op == "-" then return a - b
  else return (a * b) % 1000003 end
end
local eval
function eval(n, env)
  local k = n.k
  if k == "num" then return n.v
  elseif k == "var" then return env[n.name] or 0
  elseif k == "bin" then return evalBin(n.op, eval(n.l, env), eval(n.r, env))
  elseif k == "neg" then return -eval(n.e, env)
  else
    local args = n.args
    local vals = {}
    for i = 1, #args do vals[i] = eval(args[i], env) end
    local acc = vals[1]
    local b = builtin[n.name]
    for i = 2, #vals do acc = b(acc, vals[i]) end
    return acc
  end
end
local trees = {}
for i = 1, 60 do trees[i] = gen(5) end
local env = { a = 3, b = 5, c = 7, d = 11 }
local sum = 0
for round = 1, 160 do
  env.a = round % 17
  for i = 1, #trees do sum = (sum + eval(trees[i], env)) % 1000003 end
end
print(sum)
"##,
}];
