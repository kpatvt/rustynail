-- Havlak: loop recognition on a control flow graph (port from the public description of the Havlak benchmark of
-- are-we-fast-yet: Havlak's algorithm with union-find over a DFS numbering; LoopTesterApp builds a CFG with nested
-- reducible loops and runs the finder repeatedly).  Plain Lua 5.1-compatible (no bit ops, no integer division).
-- Output: number of loops found, sum of nesting levels, and node count of the CFG.
local N = tonumber(arg and arg[1]) or 6
local UNVISITED = 2147483647
local BB_TOP, BB_NONHEADER, BB_REDUCIBLE, BB_SELF, BB_IRREDUCIBLE, BB_DEAD = 0, 1, 2, 3, 4, 5
local MAXNONBACKPREDS = 32 * 1024
local insert, remove = table.insert, table.remove

-- CFG -----------------------------------------------------------------------------------------
local BasicBlock = {}
BasicBlock.__index = BasicBlock
local function newBB(name) return setmetatable({ name = name, inEdges = {}, outEdges = {} }, BasicBlock) end

local CFG = {}
CFG.__index = CFG
local function newCFG() return setmetatable({ nodes = {}, n = 0, edges = {}, start = nil }, CFG) end
function CFG:createNode(name)
  local node = self.nodes[name]
  if node == nil then
    node = newBB(name)
    self.nodes[name] = node
    self.n = self.n + 1
    if self.start == nil then self.start = node end
  end
  return node
end
function CFG:addEdge(from, to)
  insert(self.edges, { from, to })
  insert(from.outEdges, to)
  insert(to.inEdges, from)
end

-- Loop structure graph ---------------------------------------------------------------------------
local SimpleLoop = {}
SimpleLoop.__index = SimpleLoop
local function newLoop(bb, reducible)
  return setmetatable({ header = bb, reducible = reducible, parent = nil, children = {}, blocks = {}, nesting = 0,
    depth = 0, counter = 0, isRoot = false }, SimpleLoop)
end

local LSG = {}
LSG.__index = LSG
local function newLSG()
  local root = newLoop(nil, false)
  root.isRoot = true
  local g = setmetatable({ root = root, loops = { root }, counter = 1 }, LSG)
  root.counter = 0
  return g
end
function LSG:createNewLoop(bb, reducible)
  local loop = newLoop(bb, reducible)
  loop.counter = self.counter
  self.counter = self.counter + 1
  insert(self.loops, loop)
  return loop
end
local function calcNesting(loop, depth)
  loop.depth = depth
  for i = 1, #loop.children do calcNesting(loop.children[i], depth + 1) end
  if loop.parent ~= nil then
    local n = loop.nesting
    if loop.parent.nesting < n + 1 then loop.parent.nesting = n + 1 end
  end
end
function LSG:calculateNestingLevel()
  for i = 1, #self.loops do
    local loop = self.loops[i]
    if not loop.isRoot then
      if loop.parent == nil then loop.parent = self.root end
      insert(loop.parent.children, loop)
    end
  end
  calcNesting(self.root, 0)
end

-- Union-find node ---------------------------------------------------------------------------------
local UF = {}
UF.__index = UF
local function newUF() return setmetatable({ parent = nil, bb = nil, loop = nil, dfs = 0 }, UF) end
function UF:init(bb, dfs)
  self.parent = self
  self.bb = bb
  self.dfs = dfs
end
function UF:findSet()
  local list = {}
  local node = self
  while node ~= node.parent do
    if node.parent ~= node.parent.parent then insert(list, node) end
    node = node.parent
  end
  for i = 1, #list do list[i].parent = node.parent end
  return node
end
function UF:union(other) self.parent = other end

-- Havlak loop finder --------------------------------------------------------------------------------
local function isAncestor(w, v, last) return w <= v and v <= last[w] end

local function dfs(cur, nodes, number, last, current)
  nodes[current + 1]:init(cur, current)
  number[cur] = current
  local lastid = current
  local outs = cur.outEdges
  for i = 1, #outs do
    local target = outs[i]
    if number[target] == nil then
      lastid = dfs(target, nodes, number, last, lastid + 1)
    end
  end
  last[number[cur] + 1] = lastid
  return lastid
end

local function findLoops(cfg, lsg)
  if cfg.start == nil then return end
  local size = cfg.n
  local nonBackPreds, backPreds, header, type_, last, nodes = {}, {}, {}, {}, {}, {}
  local number = {}
  for i = 1, size do
    nonBackPreds[i] = {}
    backPreds[i] = {}
    nodes[i] = newUF()
  end
  -- DFS numbering; `last` is 1-based by dfs number + 1
  dfs(cfg.start, nodes, number, last, 0)
  local lastZ = {}
  for i = 1, size do lastZ[i - 1] = last[i] end
  last = lastZ
  for w = 0, size - 1 do
    header[w] = 0
    type_[w] = BB_NONHEADER
    local nodeW = nodes[w + 1].bb
    if nodeW == nil then
      type_[w] = BB_DEAD
    else
      local ins = nodeW.inEdges
      for i = 1, #ins do
        local v = number[ins[i]]
        if v ~= nil then
          if isAncestor(w, v, last) then insert(backPreds[w + 1], v) else insert(nonBackPreds[w + 1], v) end
        end
      end
    end
  end
  header[0] = 0
  for w = size - 1, 0, -1 do
    local nodePool = {}
    local inPool = {}
    local nodeW = nodes[w + 1].bb
    if nodeW ~= nil then
      local bp = backPreds[w + 1]
      for i = 1, #bp do
        local v = bp[i]
        if v ~= w then
          local s = nodes[v + 1]:findSet()
          if not inPool[s] then inPool[s] = true; insert(nodePool, s) end
        else
          type_[w] = BB_SELF
        end
      end
      local worklist = {}
      for i = 1, #nodePool do worklist[i] = nodePool[i] end
      if #nodePool ~= 0 then type_[w] = BB_REDUCIBLE end
      local wi = 1
      while wi <= #worklist do
        local x = worklist[wi]
        wi = wi + 1
        local nbp = nonBackPreds[x.dfs + 1]
        if #nbp > MAXNONBACKPREDS then return end
        for i = 1, #nbp do
          local y = nodes[nbp[i] + 1]
          local ydash = y:findSet()
          if not isAncestor(w, ydash.dfs, last) then
            type_[w] = BB_IRREDUCIBLE
            insert(nonBackPreds[w + 1], ydash.dfs)
          else
            if ydash.dfs ~= w and not inPool[ydash] then
              insert(worklist, ydash)
              insert(nodePool, ydash)
              inPool[ydash] = true
            end
          end
        end
      end
      if #nodePool > 0 or type_[w] == BB_SELF then
        local loop = lsg:createNewLoop(nodeW, type_[w] ~= BB_IRREDUCIBLE)
        nodes[w + 1].loop = loop
        for i = 1, #nodePool do
          local node = nodePool[i]
          header[node.dfs] = w
          node:union(nodes[w + 1])
          if node.loop ~= nil then
            node.loop.parent = loop
          else
            insert(loop.blocks, node.bb)
          end
        end
      end
    end
  end
end

-- LoopTesterApp ---------------------------------------------------------------------------------------
local cfg, lsg
local function buildDiamond(start)
  local b0 = cfg:createNode(start)
  local b1 = cfg:createNode(start + 1)
  local b2 = cfg:createNode(start + 2)
  local b3 = cfg:createNode(start + 3)
  cfg:addEdge(b0, b1)
  cfg:addEdge(b0, b2)
  cfg:addEdge(b1, b3)
  cfg:addEdge(b2, b3)
  return start + 3
end
local function buildConnect(s, e) cfg:addEdge(cfg:createNode(s), cfg:createNode(e)) end
local function buildStraight(start, n)
  for i = 0, n - 1 do buildConnect(start + i, start + i + 1) end
  return start + n
end
local function buildBaseLoop(from)
  local header = buildStraight(from, 1)
  local diamond1 = buildDiamond(header)
  local d11 = buildStraight(diamond1, 1)
  local diamond2 = buildDiamond(d11)
  local footer = buildStraight(diamond2, 1)
  buildConnect(diamond2, d11)
  buildConnect(diamond1, header)
  buildConnect(footer, from)
  return buildStraight(footer, 1)
end

local function buildCFG(parLoops, pparLoops, ppparLoops)
  cfg = newCFG()
  lsg = newLSG()
  cfg:createNode(0)
  buildBaseLoop(0)
  cfg:createNode(1)
  buildConnect(0, 2)
  -- dummy loops: run the finder over the small CFG
  for dummy = 1, 20 do findLoops(cfg, newLSG()) end
  local n = 2
  for p = 1, parLoops do
    cfg:createNode(n + 1)
    buildConnect(2, n + 1)
    n = n + 1
    for i = 1, pparLoops do
      local top = n
      n = buildStraight(n, 1)
      for j = 1, ppparLoops do n = buildBaseLoop(n) end
      local bottom = buildStraight(n, 1)
      buildConnect(n, top)
      n = bottom
    end
    buildConnect(n, 1)
  end
end

buildCFG(10, 10, 5)
local loops, levels = 0, 0
for it = 1, N do
  local g = newLSG()
  findLoops(cfg, g)
  g:calculateNestingLevel()
  loops = #g.loops
  levels = 0
  for i = 1, #g.loops do levels = levels + g.loops[i].nesting + g.loops[i].depth end
end
print("havlak", loops, levels, cfg.n)
