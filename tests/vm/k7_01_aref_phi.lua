-- K7: the loop optimiser must not turn an array address into a loop-carried value (PHI of an AREF). Sift-down of a binary heap whose left child is
-- always the smaller one (a sorted array is a heap): `i = s = l`, so the next iteration's `h[i]` is the address the previous iteration computed for
-- `h[l]`. The loop must compile (no `Compile(asm-unsupported)`) and give the interpreter's result, with and without the swap writing through a
-- metatable-free array, with a right-child variant and with mixed paths.
local function pop(h)
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

local function sorted_heap(n, rev)
  local h = { n = n }
  for i = 1, n do h[i] = { rev and (n - i) or i, i } end
  return h
end

-- (1) left path: ascending array, the popped last element (largest key) sinks to the bottom through left children only
local acc = 0
for rep = 1, 60 do
  local h = sorted_heap(300 + rep % 7)
  for _ = 1, 40 do
    local t = pop(h)
    acc = acc + t[1] * 3 + t[2]
    -- put the removed element back at the end with a large key: the next pop moves it down again
    h.n = h.n + 1
    h[h.n] = { 1000 + h.n, rep }
  end
end
print("left", acc)

-- (2) right path: the children are ordered the other way round (the array is a valid min-heap only along the right spine, so pops mix paths)
local acc2 = 0
for rep = 1, 40 do
  local h = sorted_heap(200 + rep % 5, true)
  -- heapify by repeated sift so that the heap invariant holds
  local out = {}
  for _ = 1, 30 do
    local t = pop(h)
    out[#out + 1] = t[1]
    acc2 = acc2 + t[1]
  end
  acc2 = acc2 + #out
end
print("right", acc2)

-- (3) mixed: pseudo-random keys
local seed = 12345
local function rnd(m) seed = (seed * 1103515245 + 12345) % 2147483648; return seed % m + 1 end
local h = { n = 0 }
local function push(it)
  local n = h.n + 1
  h.n = n
  h[n] = it
  while n > 1 do
    local p = n // 2
    local a, b = h[n], h[p]
    if a[1] < b[1] then h[n], h[p] = b, a; n = p else break end
  end
end
local acc3 = 0
for i = 1, 400 do push({ rnd(1000), i }) end
for i = 1, 3000 do
  local t = pop(h)
  acc3 = (acc3 + t[1] * 7 + t[2]) % 1000003
  push({ rnd(1000), i })
end
print("mixed", acc3, h.n)
