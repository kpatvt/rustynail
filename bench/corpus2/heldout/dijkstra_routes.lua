-- dijkstra_routes: imitates a road-network routing tool (Dijkstra with hand-written binary heap,
-- BFS/DFS, path reconstruction, all-pairs on a subset). Procedural style. small: N=2
local N = tonumber(arg and arg[1]) or 200

local seed = 777
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- ------------------------------------------------------------ binary heap of {dist,node}
local function heap_new() return { n = 0 } end

local function heap_push(h, item)
  local n = h.n + 1
  h.n = n
  h[n] = item
  while n > 1 do
    local p = math.floor(n / 2)
    local a, b = h[n], h[p]
    if a[1] < b[1] or (a[1] == b[1] and a[2] < b[2]) then
      h[n], h[p] = b, a
      n = p
    else
      break
    end
  end
end

local function heap_pop(h)
  local n = h.n
  if n == 0 then return nil end
  local top = h[1]
  h[1] = h[n]
  h[n] = nil
  n = n - 1
  h.n = n
  local i = 1
  while true do
    local l, r, s = i * 2, i * 2 + 1, i
    if l <= n and (h[l][1] < h[s][1] or (h[l][1] == h[s][1] and h[l][2] < h[s][2])) then s = l end
    if r <= n and (h[r][1] < h[s][1] or (h[r][1] == h[s][1] and h[r][2] < h[s][2])) then s = r end
    if s == i then break end
    h[i], h[s] = h[s], h[i]
    i = s
  end
  return top
end

-- ------------------------------------------------------------ graph construction
local V = 100 + 10 * N
local xs, ys, adj = {}, {}, {}
for i = 1, V do
  xs[i], ys[i] = rnd(1000), rnd(1000)
  adj[i] = {}
end

local edge_count = 0
local function add_road(a, b)
  if a == b then return end
  for _, e in ipairs(adj[a]) do
    if e.to == b then return end
  end
  local dx, dy = xs[a] - xs[b], ys[a] - ys[b]
  local w = math.floor(math.sqrt(dx * dx + dy * dy)) + rnd(5)
  table.insert(adj[a], { to = b, w = w })
  table.insert(adj[b], { to = a, w = w })
  edge_count = edge_count + 1
end

-- spanning chain keeps the network connected, then local shortcuts and a few highways
for i = 2, V do add_road(i - 1, i) end
for i = 1, V do
  for _ = 1, 2 do
    local best, bd = nil, 1e18
    for _ = 1, 6 do
      local j = rnd(V)
      local d = (xs[i] - xs[j]) ^ 2 + (ys[i] - ys[j]) ^ 2
      if j ~= i and d < bd then best, bd = j, d end
    end
    if best then add_road(i, best) end
  end
end
for _ = 1, math.floor(V / 20) do add_road(rnd(V), rnd(V)) end

-- ------------------------------------------------------------ algorithms
local INF = 1e15

local function dijkstra(src)
  local dist, prev = {}, {}
  for i = 1, V do dist[i] = INF end
  dist[src] = 0
  local h = heap_new()
  heap_push(h, { 0, src })
  local settled = 0
  while h.n > 0 do
    local top = heap_pop(h)
    local d, u = top[1], top[2]
    if d <= dist[u] then
      settled = settled + 1
      local es = adj[u]
      for k = 1, #es do
        local e = es[k]
        local nd = d + e.w
        if nd < dist[e.to] then
          dist[e.to] = nd
          prev[e.to] = u
          heap_push(h, { nd, e.to })
        end
      end
    end
  end
  return dist, prev, settled
end

local function path_to(prev, src, dst)
  local path = {}
  local cur = dst
  while cur do
    table.insert(path, 1, cur)
    if cur == src then return path end
    cur = prev[cur]
  end
  return nil
end

local function bfs(src)
  local hops, queue, head = { [src] = 0 }, { src }, 1
  while head <= #queue do
    local u = queue[head]
    head = head + 1
    for _, e in ipairs(adj[u]) do
      if hops[e.to] == nil then
        hops[e.to] = hops[u] + 1
        table.insert(queue, e.to)
      end
    end
  end
  return hops, #queue
end

local function dfs_order(src, limit)
  local seen, stack, order = {}, { src }, {}
  while #stack > 0 and #order < limit do
    local u = table.remove(stack)
    if not seen[u] then
      seen[u] = true
      table.insert(order, u)
      local es = adj[u]
      for k = #es, 1, -1 do
        if not seen[es[k].to] then table.insert(stack, es[k].to) end
      end
    end
  end
  return order
end

-- ------------------------------------------------------------ reports
local degree_hist = {}
for i = 1, V do
  local d = #adj[i]
  degree_hist[d] = (degree_hist[d] or 0) + 1
end
local degs = {}
for d in pairs(degree_hist) do degs[#degs + 1] = d end
table.sort(degs)
print(string.format("nodes=%d roads=%d", V, edge_count))
local parts = {}
for _, d in ipairs(degs) do parts[#parts + 1] = d .. ":" .. degree_hist[d] end
print("degrees " .. table.concat(parts, " "))

local total_settled, far_sum, hop_sum, path_sum = 0, 0, 0, 0
local longest, longest_pair = -1, nil
local samples = {}
for q = 1, N do
  local src, dst = rnd(V), rnd(V)
  local dist, prev, settled = dijkstra(src)
  total_settled = total_settled + settled
  local p = path_to(prev, src, dst)
  local hops = bfs(src)
  far_sum = far_sum + dist[dst]
  hop_sum = hop_sum + hops[dst]
  path_sum = path_sum + #p - 1
  if dist[dst] > longest then longest, longest_pair = dist[dst], { src, dst, #p } end
  if q <= 5 then
    local show = {}
    for i = 1, math.min(#p, 6) do show[#show + 1] = p[i] end
    samples[#samples + 1] = string.format("route %d->%d dist=%d hops=%d bfs=%d via %s%s", src, dst,
      dist[dst], #p - 1, hops[dst], table.concat(show, ","), #p > 6 and ",..." or "")
  end
end
for _, s in ipairs(samples) do print(s) end
print(string.format("queries=%d settled=%d dist_sum=%d bfs_hops=%d path_hops=%d", N, total_settled, far_sum, hop_sum, path_sum))
print(string.format("longest route %d->%d dist=%d nodes=%d", longest_pair[1], longest_pair[2], longest, longest_pair[3]))

-- connectivity and DFS
local _, reach = bfs(1)
print("reachable from 1:", reach)
local order = dfs_order(1, 40)
print("dfs prefix: " .. table.concat(order, " ", 1, 15))

-- all pairs on a subset of hubs
local hubs = {}
for i = 1, 12 do hubs[i] = rnd(V) end
local matrix, mx, mn, mn_pair = {}, 0, INF, nil
for i = 1, #hubs do
  local dist = dijkstra(hubs[i])
  matrix[i] = {}
  for j = 1, #hubs do
    matrix[i][j] = dist[hubs[j]]
    if i ~= j and dist[hubs[j]] > mx then mx = dist[hubs[j]] end
    if i < j and dist[hubs[j]] < mn then mn, mn_pair = dist[hubs[j]], { hubs[i], hubs[j] } end
  end
end
for i = 1, 4 do
  local row = {}
  for j = 1, 8 do row[#row + 1] = string.format("%5d", matrix[i][j]) end
  print(table.concat(row, " "))
end
print(string.format("hub diameter=%d closest pair %d-%d dist=%d", mx, mn_pair[1], mn_pair[2], mn))

-- triangle inequality spot-check over the matrix
local violations = 0
for i = 1, #hubs do
  for j = 1, #hubs do
    for k = 1, #hubs do
      if matrix[i][j] > matrix[i][k] + matrix[k][j] then violations = violations + 1 end
    end
  end
end
print("triangle violations:", violations)

-- centrality-ish summary: how often each hub lies on a route between other hubs
local counts = {}
for i = 1, #hubs do
  local _, prev = dijkstra(hubs[i])
  for j = 1, #hubs do
    local p = path_to(prev, hubs[i], hubs[j])
    for _, node in ipairs(p) do counts[node] = (counts[node] or 0) + 1 end
  end
end
local nodes = {}
for node in pairs(counts) do nodes[#nodes + 1] = node end
table.sort(nodes, function(a, b)
  if counts[a] ~= counts[b] then return counts[a] > counts[b] end
  return a < b
end)
for i = 1, 5 do print(string.format("busy node %d used %d times", nodes[i], counts[nodes[i]])) end
