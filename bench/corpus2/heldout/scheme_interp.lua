-- scheme_interp: imitates a small Scheme interpreter (tokenizer + reader, table environments with
-- parent links, eval/apply with special forms, builtins) running fib/tak/sort/map/filter. small: N=1
local N = tonumber(arg and arg[1]) or 32
local unpack = table.unpack or unpack

-- ------------------------------------------------------------ reader
local function tokenize(src)
  src = src:gsub("%(", " ( "):gsub("%)", " ) "):gsub("'", " ' ")
  local toks = {}
  for t in src:gmatch("%S+") do toks[#toks + 1] = t end
  return toks
end

local function read_all(src)
  local toks, pos = tokenize(src), 1
  local read
  function read()
    local t = toks[pos]
    pos = pos + 1
    if t == "(" then
      local list = {}
      while toks[pos] ~= ")" do list[#list + 1] = read() end
      pos = pos + 1
      return list
    elseif t == "'" then
      return { "quote", read() }
    elseif t == "#t" then return true
    elseif t == "#f" then return false
    end
    local n = tonumber(t)
    if n then return n end
    return t   -- symbol
  end
  local forms = {}
  while pos <= #toks do forms[#forms + 1] = read() end
  return forms
end

-- ------------------------------------------------------------ data
local NIL = { kind = "nil" }
local function cons(a, d) return { kind = "pair", a, d } end

local function to_list(x)
  if type(x) ~= "table" then return x end
  local l = NIL
  for i = #x, 1, -1 do l = cons(to_list(x[i]), l) end
  return l
end

local function show(v)
  local t = type(v)
  if t == "number" then return string.format("%d", v)
  elseif t == "boolean" then return v and "#t" or "#f"
  elseif t == "string" then return v
  elseif t == "function" then return "#<builtin>"
  elseif v == NIL then return "()"
  elseif v.kind == "closure" then return "#<lambda>"
  end
  local parts = {}
  while type(v) == "table" and v.kind == "pair" do
    parts[#parts + 1] = show(v[1])
    v = v[2]
  end
  if v ~= NIL then parts[#parts + 1] = "."; parts[#parts + 1] = show(v) end
  return "(" .. table.concat(parts, " ") .. ")"
end

-- ------------------------------------------------------------ environments
local function new_env(parent) return { vars = {}, parent = parent } end

local function lookup(env, name)
  local e = env
  while e do
    local v = e.vars[name]
    if v ~= nil then return v end
    e = e.parent
  end
  error("unbound variable: " .. name)
end

local function assign(env, name, value)
  local e = env
  while e do
    if e.vars[name] ~= nil then e.vars[name] = value; return end
    e = e.parent
  end
  error("set! of unbound variable: " .. name)
end

-- ------------------------------------------------------------ eval / apply
local eval
local applications = 0
local SPECIAL = {}

SPECIAL["quote"] = function(x) return false, to_list(x[2]) end

SPECIAL["if"] = function(x, env)
  if eval(x[2], env) ~= false then return true, x[3], env end
  if x[4] == nil then return false, false end
  return true, x[4], env
end

SPECIAL["define"] = function(x, env)
  local target = x[2]
  if type(target) == "table" then
    -- (define (name . params) body...)
    local params = {}
    for i = 2, #target do params[#params + 1] = target[i] end
    local body = {}
    for i = 3, #x do body[#body + 1] = x[i] end
    env.vars[target[1]] = { kind = "closure", params = params, body = body, env = env }
    return false, target[1]
  end
  env.vars[target] = eval(x[3], env)
  return false, target
end

SPECIAL["set!"] = function(x, env)
  assign(env, x[2], eval(x[3], env))
  return false, false
end

SPECIAL["lambda"] = function(x, env)
  local body = {}
  for i = 3, #x do body[#body + 1] = x[i] end
  return false, { kind = "closure", params = x[2], body = body, env = env }
end

SPECIAL["begin"] = function(x, env)
  for i = 2, #x - 1 do eval(x[i], env) end
  return true, x[#x], env
end

SPECIAL["let"] = function(x, env)
  local inner = new_env(env)
  for _, binding in ipairs(x[2]) do inner.vars[binding[1]] = eval(binding[2], env) end
  for i = 3, #x - 1 do eval(x[i], inner) end
  return true, x[#x], inner
end

SPECIAL["cond"] = function(x, env)
  for i = 2, #x do
    local clause = x[i]
    if clause[1] == "else" or eval(clause[1], env) ~= false then
      for j = 2, #clause - 1 do eval(clause[j], env) end
      return true, clause[#clause], env
    end
  end
  return false, false
end

SPECIAL["and"] = function(x, env)
  local v = true
  for i = 2, #x do
    v = eval(x[i], env)
    if v == false then return false, false end
  end
  return false, v
end

SPECIAL["or"] = function(x, env)
  for i = 2, #x do
    local v = eval(x[i], env)
    if v ~= false then return false, v end
  end
  return false, false
end

function eval(x, env)
  while true do
    local tx = type(x)
    if tx == "string" then return lookup(env, x) end
    if tx ~= "table" then return x end
    local head = x[1]
    local special = type(head) == "string" and SPECIAL[head]
    if special then
      local again, a, b = special(x, env)
      if not again then return a end
      x, env = a, b
    else
      local f = eval(head, env)
      local n = #x - 1
      local args = {}
      for i = 1, n do args[i] = eval(x[i + 1], env) end
      applications = applications + 1
      if type(f) == "function" then return f(unpack(args, 1, n)) end
      if type(f) ~= "table" or f.kind ~= "closure" then error("not a procedure: " .. show(f)) end
      local inner = new_env(f.env)
      for i = 1, #f.params do inner.vars[f.params[i]] = args[i] end
      local body = f.body
      for i = 1, #body - 1 do eval(body[i], inner) end
      x, env = body[#body], inner
    end
  end
end

-- ------------------------------------------------------------ builtins
local global = new_env(nil)
local function def(name, fn) global.vars[name] = fn end
def("+", function(a, b) return a + b end)
def("-", function(a, b) return a - b end)
def("*", function(a, b) return a * b end)
def("quotient", function(a, b) return math.floor(a / b) end)
def("remainder", function(a, b) return a % b end)
def("<", function(a, b) return a < b end)
def(">", function(a, b) return a > b end)
def("<=", function(a, b) return a <= b end)
def(">=", function(a, b) return a >= b end)
def("=", function(a, b) return a == b end)
def("eq?", function(a, b) return a == b end)
def("not", function(a) return a == false end)
def("cons", cons)
def("car", function(p) return p[1] end)
def("cdr", function(p) return p[2] end)
def("null?", function(p) return p == NIL end)
def("pair?", function(p) return type(p) == "table" and p.kind == "pair" end)
def("list", function(...)
  local l = NIL
  for i = select("#", ...), 1, -1 do l = cons((select(i, ...)), l) end
  return l
end)

-- ------------------------------------------------------------ Scheme library and benchmark programs
local PRELUDE = [[
(define (map f l) (if (null? l) '() (cons (f (car l)) (map f (cdr l)))))
(define (filter p l)
  (cond ((null? l) '())
        ((p (car l)) (cons (car l) (filter p (cdr l))))
        (else (filter p (cdr l)))))
(define (foldl f a l) (if (null? l) a (foldl f (f a (car l)) (cdr l))))
(define (append a b) (if (null? a) b (cons (car a) (append (cdr a) b))))
(define (range a b) (if (>= a b) '() (cons a (range (+ a 1) b))))
(define (length l) (if (null? l) 0 (+ 1 (length (cdr l)))))
(define (reverse l) (foldl (lambda (acc x) (cons x acc)) '() l))
(define (compose f g) (lambda (x) (f (g x))))
]]

local PROGRAMS = {
  { "fib", "(define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) (fib 15)" },
  { "tak", [[(define (tak x y z)
               (if (not (< y x)) z
                   (tak (tak (- x 1) y z) (tak (- y 1) z x) (tak (- z 1) x y))))
             (tak 12 8 4)]] },
  { "qsort", [[(define (gen n s) (if (= n 0) '() (cons (remainder s 1000) (gen (- n 1) (remainder (* s 16807) 2147483647)))))
               (define (qsort l)
                 (if (null? l) '()
                     (let ((p (car l)) (r (cdr l)))
                       (append (qsort (filter (lambda (x) (< x p)) r))
                               (cons p (qsort (filter (lambda (x) (>= x p)) r)))))))
               (define sorted (qsort (gen 120 42)))
               (list (car sorted) (length sorted) (foldl + 0 sorted))]] },
  { "sieve", [[(define (sieve l)
                 (if (null? l) '()
                     (cons (car l) (sieve (filter (lambda (x) (not (= 0 (remainder x (car l))))) (cdr l))))))
               (length (sieve (range 2 200)))]] },
  { "hof", [[(define (make-counter) (let ((n 0)) (lambda () (set! n (+ n 1)) n)))
             (define c (make-counter))
             (c) (c)
             (define inc2 (compose (lambda (x) (+ x 1)) (lambda (x) (+ x 1))))
             (list (c) (foldl + 0 (map inc2 (range 0 50))) (reverse (range 0 6)) (and 1 2) (or #f 7))]] },
}

local function run_program(src, env)
  local last
  for _, form in ipairs(read_all(src)) do last = eval(form, env) end
  return last
end

local prelude_env = global
run_program(PRELUDE, prelude_env)

local results, counts = {}, {}
for round = 1, N do
  for _, prog in ipairs(PROGRAMS) do
    local env = new_env(global)
    local before = applications
    local v = run_program(prog[2], env)
    counts[prog[1]] = (counts[prog[1]] or 0) + (applications - before)
    results[prog[1]] = show(v)
  end
end

for _, prog in ipairs(PROGRAMS) do
  print(string.format("%-6s => %-40s applications=%d", prog[1], results[prog[1]], counts[prog[1]]))
end
print("total applications:", applications)

-- reader round trip: parse a few forms and print their shapes
local sample = read_all("(define (f x) (if (< x 1) '() (cons x (f (- x 1))))) (f 4) #t")
print("forms read:", #sample)
local senv = new_env(global)
eval(sample[1], senv)
print(show(eval(sample[2], senv)))
print(show(to_list(sample[1])))
local check = 0
for _, ch in ipairs({ results.fib, results.tak, results.qsort, results.sieve, results.hof }) do
  for i = 1, #ch do check = (check * 31 + ch:byte(i)) % 1000003 end
end
print("result checksum:", check)
