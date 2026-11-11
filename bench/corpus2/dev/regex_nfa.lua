-- Small regex engine: parser to AST (. * + ? | () [] classes), Thompson NFA construction,
-- state-set simulation; many patterns over many strings. small: N=2
local N = tonumber(arg and arg[1]) or 2500

local seed = 555
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- Parser -----------------------------------------------------------------------------
-- grammar: alt := seq ('|' seq)* ; seq := rep* ; rep := atom ('*'|'+'|'?')* ; atom := char | . | (alt) | [set]
local Parser = {}
Parser.__index = Parser

function Parser.new(src) return setmetatable({ src = src, pos = 1 }, Parser) end
function Parser:peek() return self.src:sub(self.pos, self.pos) end
function Parser:next() local c = self:peek(); self.pos = self.pos + 1; return c end
function Parser:eof() return self.pos > #self.src end

function Parser:alt()
  local node = self:seq()
  while self:peek() == "|" do
    self:next()
    node = { type = "alt", node, self:seq() }
  end
  return node
end

function Parser:seq()
  local items = { type = "seq" }
  while not self:eof() and self:peek() ~= "|" and self:peek() ~= ")" do
    items[#items + 1] = self:rep()
  end
  return items
end

function Parser:rep()
  local node = self:atom()
  while true do
    local c = self:peek()
    if c == "*" then node = { type = "star", node }
    elseif c == "+" then node = { type = "plus", node }
    elseif c == "?" then node = { type = "opt", node }
    else break end
    self:next()
  end
  return node
end

function Parser:atom()
  local c = self:next()
  if c == "(" then
    local node = self:alt()
    assert(self:next() == ")", "expected )")
    return node
  elseif c == "." then
    return { type = "any" }
  elseif c == "[" then
    local negate = false
    if self:peek() == "^" then negate = true; self:next() end
    local ranges = {}
    while self:peek() ~= "]" do
      local lo = self:next()
      if self:peek() == "-" and self.src:sub(self.pos + 1, self.pos + 1) ~= "]" then
        self:next()
        ranges[#ranges + 1] = { lo, self:next() }
      else
        ranges[#ranges + 1] = { lo, lo }
      end
    end
    self:next()
    return { type = "set", negate = negate, ranges = ranges }
  elseif c == "\\" then
    return { type = "char", char = self:next() }
  end
  return { type = "char", char = c }
end

-- NFA construction ------------------------------------------------------------------------
-- states: { kind = "char"/"any"/"set"/"split"/"match", out = n, out1 = n, ... }
local function compile(ast)
  local states = {}
  local function new(s) states[#states + 1] = s; return #states end

  -- build(node, nextState) returns the entry state of the fragment
  local build
  function build(node, nxt)
    local t = node.type
    if t == "char" then
      return new { kind = "char", char = node.char, out = nxt }
    elseif t == "any" then
      return new { kind = "any", out = nxt }
    elseif t == "set" then
      return new { kind = "set", negate = node.negate, ranges = node.ranges, out = nxt }
    elseif t == "seq" then
      local entry = nxt
      for i = #node, 1, -1 do entry = build(node[i], entry) end
      return entry
    elseif t == "alt" then
      local a, b = build(node[1], nxt), build(node[2], nxt)
      return new { kind = "split", out = a, out1 = b }
    elseif t == "opt" then
      return new { kind = "split", out = build(node[1], nxt), out1 = nxt }
    elseif t == "star" then
      local split = new { kind = "split", out1 = nxt }
      states[split].out = build(node[1], split)
      return split
    elseif t == "plus" then
      local split = new { kind = "split", out1 = nxt }
      local entry = build(node[1], split)
      states[split].out = entry
      return entry
    end
    error("unknown node " .. tostring(t))
  end

  local matchState = new { kind = "match" }
  local start = build(ast, matchState)
  return { states = states, start = start }
end

-- Simulation ------------------------------------------------------------------------------
local function addState(nfa, list, onList, s, gen)
  if onList[s] == gen then return end
  onList[s] = gen
  local st = nfa.states[s]
  if st.kind == "split" then
    addState(nfa, list, onList, st.out, gen)
    addState(nfa, list, onList, st.out1, gen)
  else
    list[#list + 1] = s
  end
end

local function setMatches(st, c)
  local hit = false
  for _, r in ipairs(st.ranges) do
    if c >= r[1] and c <= r[2] then hit = true; break end
  end
  return hit ~= st.negate
end

local function match(nfa, input)
  local gen = 0
  local onList = {}
  local clist, nlist = {}, {}
  gen = gen + 1
  addState(nfa, clist, onList, nfa.start, gen)
  for i = 1, #input do
    local c = input:sub(i, i)
    gen = gen + 1
    nlist = {}
    for _, s in ipairs(clist) do
      local st = nfa.states[s]
      local k = st.kind
      if (k == "char" and st.char == c) or k == "any" or (k == "set" and setMatches(st, c)) then
        addState(nfa, nlist, onList, st.out, gen)
      end
    end
    clist = nlist
    if #clist == 0 then return false end
  end
  for _, s in ipairs(clist) do
    if nfa.states[s].kind == "match" then return true end
  end
  return false
end

local function Regex(src)
  local p = Parser.new(src)
  local ast = p:alt()
  assert(p:eof(), "trailing input in " .. src)
  return compile(ast)
end

-- Workload -----------------------------------------------------------------------------------
local patterns = {
  "abc", "a.c", "ab*c", "(ab)+c?", "[a-c]+d", "x(y|z)*w", "[^ab]*b", "(a|b)*abb",
  "h(el+)o", "[0-9]+(\\.[0-9]+)?", "(foo|bar)+baz?", "a?a?a?aaa", "((a|b)(c|d))*", ".*q.*", "[a-f0-9]+x?",
}
local compiled = {}
for i, p in ipairs(patterns) do compiled[i] = Regex(p) end

local alphabet = "abcdwxyzhelo09.fqb"
local function randomString(len)
  local t = {}
  for i = 1, len do
    local k = rnd(#alphabet)
    t[i] = alphabet:sub(k, k)
  end
  return table.concat(t)
end

local inputs = { "abc", "abbbc", "ababc", "aabd", "xyzyzw", "helllo", "3.14", "foobarbaz", "aaaaaa", "abcdabcd", "qqq", "abb" }
for i = 1, N do inputs[#inputs + 1] = randomString(3 + i % 12) end

local matchCounts = {}
local total, states = 0, 0
for pi, nfa in ipairs(compiled) do
  local c = 0
  states = states + #nfa.states
  for _, s in ipairs(inputs) do
    if match(nfa, s) then c = c + 1 end
  end
  matchCounts[pi] = c
  total = total + c
end
for pi, p in ipairs(patterns) do
  print(string.format("%-18s states=%2d matches=%d/%d", p, #compiled[pi].states, matchCounts[pi], #inputs))
end
print("total matches " .. total .. ", total states " .. states)

-- Stress: long strings against pathological pattern ((a?)^k a^k)
local k = 12
local pat = string.rep("a?", k) .. string.rep("a", k)
local nfa = Regex(pat)
local ok = 0
for len = 1, 2 * k + 2 do
  if match(nfa, string.rep("a", len)) then ok = ok + 1 end
end
print("pathological accepted lengths: " .. ok)

local longInput = string.rep("ab", 100 * N)
print("(a|b)*abb on long: " .. tostring(match(compiled[8], longInput .. "abb")) .. " " .. tostring(match(compiled[8], longInput)))
