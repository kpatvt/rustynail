-- sudoku_solver: imitates a puzzle-solver script: constraint propagation + backtracking sudoku,
-- plus N-queens counting and a knapsack DP. Index-based cells, candidate tables. small: N=1
local N = tonumber(arg and arg[1]) or 420

local seed = 271828
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- ------------------------------------------------------------ board geometry
local UNITS, UNITS_OF, PEERS = {}, {}, {}
for i = 1, 81 do UNITS_OF[i] = {} end

local function add_unit(cells)
  UNITS[#UNITS + 1] = cells
  for _, c in ipairs(cells) do table.insert(UNITS_OF[c], cells) end
end
for r = 0, 8 do
  local row, col = {}, {}
  for c = 0, 8 do
    row[#row + 1] = r * 9 + c + 1
    col[#col + 1] = c * 9 + r + 1
  end
  add_unit(row)
  add_unit(col)
end
for br = 0, 2 do
  for bc = 0, 2 do
    local box = {}
    for r = 0, 2 do
      for c = 0, 2 do box[#box + 1] = (br * 3 + r) * 9 + bc * 3 + c + 1 end
    end
    add_unit(box)
  end
end
for i = 1, 81 do
  local seen, list = {}, {}
  for _, u in ipairs(UNITS_OF[i]) do
    for _, c in ipairs(u) do
      if c ~= i and not seen[c] then seen[c] = true; list[#list + 1] = c end
    end
  end
  table.sort(list)
  PEERS[i] = list
end

-- ------------------------------------------------------------ propagation
local assign

local function eliminate(st, i, d)
  local ci = st.c[i]
  if not ci[d] then return true end
  ci[d] = false
  local n = st.n[i] - 1
  st.n[i] = n
  if n == 0 then return false end
  if n == 1 then
    local only
    for k = 1, 9 do
      if ci[k] then only = k; break end
    end
    local peers = PEERS[i]
    for k = 1, #peers do
      if not eliminate(st, peers[k], only) then return false end
    end
  end
  for _, unit in ipairs(UNITS_OF[i]) do
    local place, cnt = nil, 0
    for k = 1, 9 do
      local cell = unit[k]
      if st.c[cell][d] then cnt = cnt + 1; place = cell end
    end
    if cnt == 0 then return false end
    if cnt == 1 and not assign(st, place, d) then return false end
  end
  return true
end

function assign(st, i, d)
  local ci = st.c[i]
  for k = 1, 9 do
    if k ~= d and ci[k] then
      if not eliminate(st, i, k) then return false end
    end
  end
  return true
end

local function new_state()
  local st = { c = {}, n = {} }
  for i = 1, 81 do
    st.c[i] = { true, true, true, true, true, true, true, true, true }
    st.n[i] = 9
  end
  return st
end

local function copy_state(st)
  local ns = { c = {}, n = {} }
  for i = 1, 81 do
    local src, dst = st.c[i], {}
    for k = 1, 9 do dst[k] = src[k] end
    ns.c[i] = dst
    ns.n[i] = st.n[i]
  end
  return ns
end

local nodes = 0
local function search(st)
  nodes = nodes + 1
  local best, bn = nil, 10
  for i = 1, 81 do
    local n = st.n[i]
    if n > 1 and n < bn then best, bn = i, n end
  end
  if not best then return st end
  for d = 1, 9 do
    if st.c[best][d] then
      local trial = copy_state(st)
      if assign(trial, best, d) then
        local r = search(trial)
        if r then return r end
      end
    end
  end
  return nil
end

local function parse_puzzle(s)
  local st = new_state()
  for i = 1, 81 do
    local ch = s:sub(i, i)
    if ch:match("[1-9]") then
      if not assign(st, i, tonumber(ch)) then return nil end
    end
  end
  return st
end

local function solution_string(st)
  local out = {}
  for i = 1, 81 do
    local d = 0
    for k = 1, 9 do if st.c[i][k] then d = k end end
    out[i] = tostring(d)
  end
  return table.concat(out)
end

-- ------------------------------------------------------------ puzzle generation
local function shuffled(n)
  local t = {}
  for i = 1, n do t[i] = i end
  for i = n, 2, -1 do
    local j = rnd(i)
    t[i], t[j] = t[j], t[i]
  end
  return t
end

local function generate_puzzle(holes)
  local digits = shuffled(9)
  local rows, cols = {}, {}
  local bands, stacks = shuffled(3), shuffled(3)
  for b = 1, 3 do
    local within = shuffled(3)
    for k = 1, 3 do
      rows[#rows + 1] = (bands[b] - 1) * 3 + within[k] - 1
    end
    within = shuffled(3)
    for k = 1, 3 do
      cols[#cols + 1] = (stacks[b] - 1) * 3 + within[k] - 1
    end
  end
  local cells = {}
  for r = 1, 9 do
    for c = 1, 9 do
      local rr, cc = rows[r], cols[c]
      cells[(r - 1) * 9 + c] = tostring(digits[(rr * 3 + math.floor(rr / 3) + cc) % 9 + 1])
    end
  end
  local order = shuffled(81)
  for k = 1, holes do cells[order[k]] = "." end
  return table.concat(cells)
end

-- ------------------------------------------------------------ N-queens and knapsack
local function queens(n)
  local cols, d1, d2, count = {}, {}, {}, 0
  local function place(r)
    if r > n then count = count + 1; return end
    for c = 1, n do
      if not cols[c] and not d1[r + c] and not d2[r - c + n] then
        cols[c], d1[r + c], d2[r - c + n] = true, true, true
        place(r + 1)
        cols[c], d1[r + c], d2[r - c + n] = nil, nil, nil
      end
    end
  end
  place(1)
  return count
end

local function knapsack(items, cap)
  local best = {}
  for w = 0, cap do best[w] = 0 end
  local take = {}
  for i, it in ipairs(items) do
    take[i] = {}
    for w = cap, it.w, -1 do
      local cand = best[w - it.w] + it.v
      if cand > best[w] then
        best[w] = cand
        take[i][w] = true
      end
    end
  end
  local chosen, w = {}, cap
  for i = #items, 1, -1 do
    if take[i][w] then
      chosen[#chosen + 1] = i
      w = w - items[i].w
    end
  end
  return best[cap], chosen
end

-- ------------------------------------------------------------ main
local KNOWN = {
  "4.....8.5.3..........7......2.....6.....8.4......1.......6.3.7.5..2.....1.4......",
  "..3.2.6..9..3.5..1..18.64....81.29..7.......8..67.82....26.95..8..2.3..9..5.1.3..",
  "8..........36......7..9.2...5...7.......457.....1...3...1....68..85...1..9....4..",
}
local puzzles = {}
for k = 1, (N >= 3 and 3 or 1) do puzzles[#puzzles + 1] = KNOWN[k] end
for i = 1, N * 2 do puzzles[#puzzles + 1] = generate_puzzle(46 + rnd(14)) end

local solved, failed, givens = 0, 0, 0
local checksum = 0
for pi, p in ipairs(puzzles) do
  local before = nodes
  local st = parse_puzzle(p)
  local result = st and search(st)
  local _, g = p:gsub("%d", "")
  givens = givens + g
  if result then
    solved = solved + 1
    local s = solution_string(result)
    for i = 1, 81 do checksum = (checksum * 10 + s:byte(i) - 48) % 999999937 end
    if pi <= 4 then
      print(string.format("puzzle %d (%d givens, %d nodes)", pi, g, nodes - before))
      for r = 0, 2 do print("  " .. s:sub(r * 9 + 1, r * 9 + 9)) end
    end
  else
    failed = failed + 1
  end
end
print(string.format("puzzles=%d solved=%d failed=%d givens=%d search nodes=%d checksum=%d", #puzzles, solved, failed,
  givens, nodes, checksum))

-- a contradictory puzzle must be rejected
local bad = "55" .. string.rep(".", 79)
print("contradiction detected:", parse_puzzle(bad) == nil)

local qn = N >= 20 and 9 or (N >= 4 and 8 or 6)
print(string.format("queens(%d) = %d; queens(5) = %d", qn, queens(qn), queens(5)))

local items = {}
for i = 1, 25 + N do items[i] = { w = rnd(30) + 3, v = rnd(90) + 10 } end
local cap = 100 + N * 12
local value, chosen = knapsack(items, cap)
local wsum = 0
for _, i in ipairs(chosen) do wsum = wsum + items[i].w end
table.sort(chosen)
print(string.format("knapsack: items=%d cap=%d best=%d chosen=%d weight=%d", #items, cap, value, #chosen, wsum))
print("chosen head: " .. table.concat(chosen, ",", 1, math.min(10, #chosen)))
