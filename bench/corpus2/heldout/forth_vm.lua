-- forth_vm: imitates a toy Forth: outer interpreter (tokenizer), dictionary of words (Lua
-- primitives and colon definitions compiled to instruction arrays), IF/ELSE/THEN, DO/LOOP,
-- BEGIN/UNTIL, variables. small: N=1
local N = tonumber(arg and arg[1]) or 36

local stack, rstack, memory = {}, {}, {}
local dict = {}
local output = {}
local steps = 0

local function push(v) stack[#stack + 1] = v end
local function pop()
  local n = #stack
  if n == 0 then error("stack underflow") end
  local v = stack[n]
  stack[n] = nil
  return v
end

-- ------------------------------------------------------------ primitive words
local prim = {}
prim["+"]    = function() local b, a = pop(), pop(); push(a + b) end
prim["-"]    = function() local b, a = pop(), pop(); push(a - b) end
prim["*"]    = function() local b, a = pop(), pop(); push(a * b) end
prim["/"]    = function() local b, a = pop(), pop(); push(math.floor(a / b)) end
prim["MOD"]  = function() local b, a = pop(), pop(); push(a % b) end
prim["="]    = function() local b, a = pop(), pop(); push(a == b and 1 or 0) end
prim["<"]    = function() local b, a = pop(), pop(); push(a < b and 1 or 0) end
prim[">"]    = function() local b, a = pop(), pop(); push(a > b and 1 or 0) end
prim["AND"]  = function() local b, a = pop(), pop(); push((a ~= 0 and b ~= 0) and 1 or 0) end
prim["OR"]   = function() local b, a = pop(), pop(); push((a ~= 0 or b ~= 0) and 1 or 0) end
prim["NOT"]  = function() push(pop() == 0 and 1 or 0) end
prim["DUP"]  = function() local a = pop(); push(a); push(a) end
prim["DROP"] = function() pop() end
prim["SWAP"] = function() local b, a = pop(), pop(); push(b); push(a) end
prim["OVER"] = function() local b, a = pop(), pop(); push(a); push(b); push(a) end
prim["ROT"]  = function() local c, b, a = pop(), pop(), pop(); push(b); push(c); push(a) end
prim["2DUP"] = function() local b, a = pop(), pop(); push(a); push(b); push(a); push(b) end
prim["@"]    = function() push(memory[pop()] or 0) end
prim["!"]    = function() local addr, v = pop(), pop(); memory[addr] = v end
prim["."]    = function() output[#output + 1] = string.format("%d", pop()) end
prim["CR"]   = function() output[#output + 1] = "\n" end
prim["I"]    = function() push(rstack[#rstack]) end
prim["J"]    = function() push(rstack[#rstack - 2]) end
prim["NEGATE"] = function() push(-pop()) end
prim["ABS"]  = function() local a = pop(); push(a < 0 and -a or a) end
prim["MAX"]  = function() local b, a = pop(), pop(); push(a > b and a or b) end
prim["MIN"]  = function() local b, a = pop(), pop(); push(a < b and a or b) end

for name, fn in pairs(prim) do dict[name] = { kind = "prim", fn = fn } end

-- ------------------------------------------------------------ inner interpreter
local execute

-- instruction handlers: return the next pc
local OPS = {}
OPS.lit  = function(ins, pc) push(ins[2]); return pc + 1 end
OPS.call = function(ins, pc) execute(ins[2]); return pc + 1 end
OPS.jz   = function(ins, pc) if pop() == 0 then return ins[2] end return pc + 1 end
OPS.jmp  = function(ins, pc) return ins[2] end
OPS["do"] = function(ins, pc)
  local start, limit = pop(), pop()
  if start >= limit then return ins[2] end          -- empty loop: skip past LOOP
  rstack[#rstack + 1] = limit
  rstack[#rstack + 1] = start
  return pc + 1
end
OPS.loop = function(ins, pc)
  local n = #rstack
  local idx = rstack[n] + 1
  if idx < rstack[n - 1] then
    rstack[n] = idx
    return ins[2]
  end
  rstack[n] = nil
  rstack[n - 1] = nil
  return pc + 1
end
OPS.var = function(ins, pc) push(ins[2]); return pc + 1 end

function execute(word)
  steps = steps + 1
  if word.kind == "prim" then
    word.fn()
  elseif word.kind == "colon" then
    local code, pc = word.code, 1
    while pc <= #code do
      local ins = code[pc]
      pc = OPS[ins[1]](ins, pc)
      steps = steps + 1
    end
  elseif word.kind == "var" then
    push(word.addr)
  end
end

-- ------------------------------------------------------------ compiler (immediate words)
local compiling, current, ctl = false, nil, {}
local function emit(op, arg) current.code[#current.code + 1] = { op, arg }; return #current.code end

local IMMEDIATE = {}
IMMEDIATE["IF"] = function() ctl[#ctl + 1] = emit("jz", 0) end
IMMEDIATE["ELSE"] = function()
  local j = emit("jmp", 0)
  local ifpos = table.remove(ctl)
  current.code[ifpos][2] = #current.code + 1
  ctl[#ctl + 1] = j
end
IMMEDIATE["THEN"] = function()
  local pos = table.remove(ctl)
  current.code[pos][2] = #current.code + 1
end
IMMEDIATE["DO"] = function()
  ctl[#ctl + 1] = emit("do", 0)
end
IMMEDIATE["LOOP"] = function()
  local dopos = table.remove(ctl)
  emit("loop", dopos + 1)
  current.code[dopos][2] = #current.code + 1
end
IMMEDIATE["BEGIN"] = function() ctl[#ctl + 1] = #current.code + 1 end
IMMEDIATE["UNTIL"] = function() emit("jz", table.remove(ctl)) end

-- ------------------------------------------------------------ outer interpreter
local next_addr = 100

local function interpret(source)
  local tokens = {}
  for tok in source:gmatch("%S+") do tokens[#tokens + 1] = tok:upper() end
  local i = 1
  while i <= #tokens do
    local tok = tokens[i]
    i = i + 1
    if tok == "(" then
      while tokens[i] ~= ")" do i = i + 1 end
      i = i + 1
    elseif tok == ":" then
      current = { kind = "colon", name = tokens[i], code = {} }
      dict[current.name] = current   -- visible immediately so words can recurse
      i = i + 1
      compiling = true
    elseif tok == ";" then
      dict[current.name] = current
      compiling = false
    elseif tok == "VARIABLE" then
      dict[tokens[i]] = { kind = "var", addr = next_addr }
      memory[next_addr] = 0
      next_addr = next_addr + 1
      i = i + 1
    elseif compiling and IMMEDIATE[tok] then
      IMMEDIATE[tok]()
    else
      local word = dict[tok]
      local num = tonumber(tok)
      if word then
        if compiling then emit("call", word) else execute(word) end
      elseif num then
        if compiling then emit("lit", num) else push(num) end
      else
        error("unknown word: " .. tok)
      end
    end
  end
end

-- ------------------------------------------------------------ benchmark programs
local DEFS = [[
( recursive fibonacci )
: FIB DUP 2 < IF ELSE DUP 1 - FIB SWAP 2 - FIB + THEN ;
: SUMSQ 0 SWAP 0 DO I I * + LOOP ;
: PRIME? 1 SWAP DUP 2 DO DUP I MOD 0 = IF SWAP DROP 0 SWAP THEN LOOP DROP ;
: COUNTPRIMES 0 SWAP 2 DO I PRIME? + LOOP ;
: STEP DUP 2 MOD 0 = IF 2 / ELSE 3 * 1 + THEN ;
: COLLATZ 0 SWAP BEGIN SWAP 1 + SWAP STEP DUP 1 = UNTIL DROP ;
: TOTAL 0 SWAP 1 DO I COLLATZ + LOOP ;
VARIABLE ACC
: ACCUM ACC @ + ACC ! ;
: TABLE 0 ACC ! 30 1 DO 30 1 DO I J * ACCUM LOOP LOOP ACC @ ;
: GCD BEGIN DUP 0 = IF DROP 1 ELSE SWAP OVER MOD 0 THEN UNTIL ;
: GCDSUM 0 SWAP 1 DO 360 I GCD + LOOP ;
]]

interpret(DEFS)
local words = {}
for name, w in pairs(dict) do if w.kind ~= "prim" then words[#words + 1] = name end end
table.sort(words)
print("defined words: " .. table.concat(words, " "))

local function code_size(name) return #dict[name].code end
print(string.format("code sizes: FIB=%d PRIME?=%d COLLATZ=%d TABLE=%d", code_size("FIB"), code_size("PRIME?"),
  code_size("COLLATZ"), code_size("TABLE")))

local function bench(label, src)
  local before = steps
  output = {}
  interpret(src)
  local depth = #stack
  print(string.format("%-10s -> %-10s steps=%d depth=%d", label, table.concat(output, " "), steps - before, depth))
end

bench("fib", string.format("%d FIB .", 10 + math.floor(N / 8)))
bench("sumsq", string.format("%d SUMSQ .", 200 * N))
bench("primes", string.format("%d COUNTPRIMES .", 40 * N))
bench("collatz", string.format("27 COLLATZ . %d TOTAL .", 8 * N))
bench("table", "TABLE .")
bench("gcd", string.format("%d GCDSUM .", 20 * N))
bench("mixed", "3 4 + 5 * . 10 3 MOD . 7 NEGATE ABS . 2 9 MAX . 1 2 3 ROT . . . CR")
print("memory ACC:", memory[dict["ACC"].addr])
print("total steps:", steps)

-- tiny stress: define words at run time, in a loop, and call them
local checksum = 0
for k = 1, N do
  interpret(string.format(": W%d DUP * %d + ; %d W%d", k % 7, k, k, k % 7))
  checksum = (checksum + pop()) % 1000003
end
print("define/call checksum:", checksum)
