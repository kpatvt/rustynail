-- Jumper (Yonaba) style pathfinding: Grid + Node classes with metatable-based OOP, a binary heap keyed by node
-- f-cost (comparator through __lt), heuristics table, A* and Dijkstra search, path smoothing/filling.
local sqrt, abs, max, min, floor = math.sqrt, math.abs, math.max, math.min, math.floor

-- Node
local Node = {}
Node.__index = Node
function Node:new(x, y) return setmetatable({ _x = x, _y = y, _clearance = {} }, Node) end
function Node.__lt(A, B) return (A._f < B._f) end
function Node:getX() return self._x end
function Node:getY() return self._y end
function Node:getPos() return self._x, self._y end
function Node:reset()
  self._g, self._h, self._f = nil, nil, nil
  self._opened, self._closed, self._parent = nil, nil, nil
  return self
end
setmetatable(Node, { __call = function(self, ...) return Node:new(...) end })

-- Heap
local function f_min(a, b) return a < b end
local function percolate_up(heap, index)
  if index == 1 then return end
  local pIndex
  if index <= 1 then return end
  if index % 2 == 0 then pIndex = index / 2 else pIndex = (index - 1) / 2 end
  if not heap._sort(heap._heap[pIndex], heap._heap[index]) then
    heap._heap[pIndex], heap._heap[index] = heap._heap[index], heap._heap[pIndex]
    percolate_up(heap, pIndex)
  end
end
local function percolate_down(heap, index)
  local lfIndex, rtIndex, minIndex
  lfIndex = 2 * index
  rtIndex = lfIndex + 1
  if rtIndex > heap._size then
    if lfIndex > heap._size then return
    else minIndex = lfIndex end
  else
    if heap._sort(heap._heap[lfIndex], heap._heap[rtIndex]) then minIndex = lfIndex else minIndex = rtIndex end
  end
  if not heap._sort(heap._heap[index], heap._heap[minIndex]) then
    heap._heap[index], heap._heap[minIndex] = heap._heap[minIndex], heap._heap[index]
    percolate_down(heap, minIndex)
  end
end
local heap = setmetatable({}, { __call = function(self, ...) return self:new(...) end })
heap.__index = heap
function heap:new(comp) return setmetatable({ _heap = {}, _sort = comp or f_min, _size = 0 }, heap) end
function heap:empty() return (self._size == 0) end
function heap:clear() self._heap = {}; self._size = 0; self._sort = self._sort or f_min; return self end
function heap:push(item)
  if item then
    self._size = self._size + 1
    self._heap[self._size] = item
    percolate_up(self, self._size)
  end
  return self
end
function heap:pop()
  local root
  if self._size > 0 then
    root = self._heap[1]
    self._heap[1] = self._heap[self._size]
    self._heap[self._size] = nil
    self._size = self._size - 1
    if self._size > 1 then percolate_down(self, 1) end
  end
  return root
end
function heap:heapify(item)
  if self._size == 0 then return end
  if item then
    local i
    for k = 1, self._size do if self._heap[k] == item then i = k break end end
    if i then
      percolate_down(self, i)
      percolate_up(self, i)
    end
    return self
  end
  for i = floor(self._size / 2), 1, -1 do percolate_down(self, i) end
  return self
end

-- Heuristics
local Heuristics = {}
function Heuristics.MANHATTAN(nodeA, nodeB)
  local dx = abs(nodeA._x - nodeB._x)
  local dy = abs(nodeA._y - nodeB._y)
  return (dx + dy)
end
function Heuristics.EUCLIDIAN(nodeA, nodeB)
  local dx = nodeA._x - nodeB._x
  local dy = nodeA._y - nodeB._y
  return sqrt(dx * dx + dy * dy)
end
function Heuristics.DIAGONAL(nodeA, nodeB)
  local dx = abs(nodeA._x - nodeB._x)
  local dy = abs(nodeA._y - nodeB._y)
  return max(dx, dy)
end

-- Grid
local straightOffsets = { { x = 1, y = 0 }, { x = -1, y = 0 }, { x = 0, y = 1 }, { x = 0, y = -1 } }
local diagonalOffsets = { { x = -1, y = -1 }, { x = 1, y = -1 }, { x = -1, y = 1 }, { x = 1, y = 1 } }
local Grid = {}
Grid.__index = Grid
function Grid:new(map)
  local g = setmetatable({ _map = map, _nodes = {}, _min_x = 1, _min_y = 1, _max_x = #map[1], _max_y = #map }, Grid)
  for y = 1, #map do
    g._nodes[y] = {}
    for x = 1, #map[1] do g._nodes[y][x] = Node(x, y) end
  end
  return g
end
function Grid:isWalkableAt(x, y, walkable)
  local row = self._map[y]
  local nodeValue = row and row[x]
  if nodeValue then
    if not walkable then return true end
    if type(walkable) == "function" then return walkable(nodeValue) end
    return nodeValue == walkable
  end
  return false
end
function Grid:getNodeAt(x, y)
  local row = self._nodes[y]
  return row and row[x] or nil
end
function Grid:getNeighbours(node, walkable, allowDiagonal, tunnel)
  local neighbours = {}
  for i = 1, #straightOffsets do
    local n = self:getNodeAt(node._x + straightOffsets[i].x, node._y + straightOffsets[i].y)
    if n and self:isWalkableAt(n._x, n._y, walkable) then neighbours[#neighbours + 1] = n end
  end
  if not allowDiagonal then return neighbours end
  for i = 1, #diagonalOffsets do
    local n = self:getNodeAt(node._x + diagonalOffsets[i].x, node._y + diagonalOffsets[i].y)
    if n and self:isWalkableAt(n._x, n._y, walkable) then
      if tunnel then
        neighbours[#neighbours + 1] = n
      else
        local skipThisNode = false
        local n1 = self:getNodeAt(node._x + diagonalOffsets[i].x, node._y)
        local n2 = self:getNodeAt(node._x, node._y + diagonalOffsets[i].y)
        if ((n1 and n2) and not self:isWalkableAt(n1._x, n1._y, walkable) and not self:isWalkableAt(n2._x, n2._y, walkable)) then
          skipThisNode = true
        end
        if not skipThisNode then neighbours[#neighbours + 1] = n end
      end
    end
  end
  return neighbours
end

-- A*
local function computeCost(node, neighbour, finder)
  local mCost = Heuristics.EUCLIDIAN(neighbour, node)
  if node._g + mCost < neighbour._g then
    neighbour._parent = node
    neighbour._g = node._g + mCost
  end
end
local function updateVertex(finder, openList, node, neighbour, endNode, heuristic)
  local oldG = neighbour._g
  computeCost(node, neighbour, finder)
  if neighbour._g < oldG then
    if neighbour._opened then neighbour._opened = false end
    neighbour._h = heuristic(endNode, neighbour)
    neighbour._f = neighbour._g + neighbour._h
    openList:push(neighbour)
    neighbour._opened = true
  end
end
local function astar_search(finder, startNode, endNode, toClear, tunnel)
  local heuristic = finder._heuristic
  local openList = heap()
  startNode._g = 0
  startNode._h = heuristic(endNode, startNode)
  startNode._f = startNode._g + startNode._h
  openList:push(startNode)
  toClear[startNode] = true
  startNode._opened = true
  while not openList:empty() do
    local node = openList:pop()
    if not node._closed then
      node._closed = true
      if node == endNode then return node end
      local neighbours = finder._grid:getNeighbours(node, finder._walkable, finder._allowDiagonal, tunnel)
      for i = 1, #neighbours do
        local neighbour = neighbours[i]
        if not neighbour._closed then
          toClear[neighbour] = true
          if not neighbour._opened then
            neighbour._g = math.huge
            neighbour._parent = nil
          end
          updateVertex(finder, openList, node, neighbour, endNode, heuristic)
        end
      end
    end
  end
  return nil
end

local Path = {}
Path.__index = Path
function Path:new() return setmetatable({ _nodes = {} }, Path) end
function Path:iter()
  local i, pathLen = 1, #self._nodes
  return function()
    if self._nodes[i] then
      i = i + 1
      return self._nodes[i - 1], i - 1
    end
  end
end
function Path:getLength()
  local len = 0
  for i = 2, #self._nodes do len = len + Heuristics.EUCLIDIAN(self._nodes[i], self._nodes[i - 1]) end
  return len
end
function Path:filter()
  local i = 2
  local xi, yi, dx, dy, olddx, olddy
  xi, yi = self._nodes[i]._x, self._nodes[i]._y
  dx, dy = xi - self._nodes[i - 1]._x, yi - self._nodes[i - 1]._y
  while true do
    olddx, olddy = dx, dy
    if self._nodes[i + 1] then
      i = i + 1
      xi, yi = self._nodes[i]._x, self._nodes[i]._y
      dx, dy = xi - self._nodes[i - 1]._x, yi - self._nodes[i - 1]._y
      if olddx == dx and olddy == dy then
        table.remove(self._nodes, i - 1)
        i = i - 1
      end
    else break end
  end
  return self
end

local function traceBackPath(finder, node, startNode)
  local path = Path:new()
  path._grid = finder._grid
  while true do
    if node._parent then
      table.insert(path._nodes, 1, node)
      node = node._parent
    else
      table.insert(path._nodes, 1, startNode)
      return path
    end
  end
end

local Pathfinder = {}
Pathfinder.__index = Pathfinder
function Pathfinder:new(grid, walkable)
  return setmetatable({ _grid = grid, _walkable = walkable, _heuristic = Heuristics.MANHATTAN, _allowDiagonal = true }, Pathfinder)
end
function Pathfinder:setHeuristic(name) self._heuristic = Heuristics[name]; return self end
function Pathfinder:getPath(startX, startY, endX, endY)
  self:reset()
  local startNode = self._grid:getNodeAt(startX, startY)
  local endNode = self._grid:getNodeAt(endX, endY)
  local _endNode = astar_search(self, startNode, endNode, self._toClear)
  if _endNode then return traceBackPath(self, _endNode, startNode) end
  return nil
end
function Pathfinder:reset()
  for node in pairs(self._toClear or {}) do node:reset() end
  self._toClear = {}
  return self
end

-- Workload: a maze-ish map
local W, H = 60, 60
local seed = 99
local function rnd(n) seed = (seed * 16807) % 2147483647; return floor(seed / 65536) % n end
local map = {}
for y = 1, H do
  map[y] = {}
  for x = 1, W do
    local wall = (rnd(100) < 22) or (x % 10 == 0 and y % 7 ~= 0)
    map[y][x] = wall and 1 or 0
  end
end
map[1][1], map[H][W] = 0, 0
local grid = Grid:new(map)
local finder = Pathfinder:new(grid, 0)

local N = tonumber(arg and arg[1]) or 70
local found, total_len, steps = 0, 0, 0
local heur = { "MANHATTAN", "EUCLIDIAN", "DIAGONAL" }
for r = 1, N do
  finder:setHeuristic(heur[1 + r % 3])
  local sx, sy = 1 + rnd(W), 1 + rnd(H)
  local ex, ey = 1 + rnd(W), 1 + rnd(H)
  map[sy][sx], map[ey][ex] = 0, 0
  local path = finder:getPath(sx, sy, ex, ey)
  if path then
    found = found + 1
    total_len = total_len + path:getLength()
    for node, count in path:iter() do steps = steps + 1 end
    path:filter()
    steps = steps + #path._nodes
  end
  local p2 = finder:getPath(1, 1, W, H)
  if p2 then total_len = total_len + p2:getLength() end
end
print(string.format("found %d steps %d length %.4f", found, steps, total_len))
