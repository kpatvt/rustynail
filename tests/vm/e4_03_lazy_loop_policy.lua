-- E4 (docs/JIT_TRACE.md section 35.1): recursive functions with loops whose first root trace is not the function-entry trace ending in the
-- self-call (a loop trace, a leaf path) stay eager: their regions would end at every call (the leaves return through exits). The policy
-- test asserts that none of these prototypes becomes lazy-aware; the outputs are the interpreter's either way.

-- queens with the counter in an upvalue: the loop over the columns is the first trace of `place`
local N = 8
local count = 0
local cols, d1, d2 = {}, {}, {}
local function place(r)
  if r > N then count = count + 1 return end
  for c = 1, N do
    if not cols[c] and not d1[r + c] and not d2[r - c + N] then
      cols[c], d1[r + c], d2[r - c + N] = true, true, true
      place(r + 1)
      cols[c], d1[r + c], d2[r - c + N] = false, false, false
    end
  end
end
for i = 1, 6 do place(1) end
print("queens", count)

-- a tree of arrays whose leaves fill an array in a loop (the leaf loop is the first trace)
local seed = 74755
local function rnd() seed = (seed * 1309 + 13849) % 65536 return seed end
local cnt
local function build(depth)
  cnt = cnt + 1
  if depth == 1 then local a = {} for i = 1, rnd() % 10 + 1 do a[i] = false end return a end
  local arr = {}
  for i = 1, 4 do arr[i] = build(depth - 1) end
  return arr
end
local tot = 0
for it = 1, 20 do cnt = 0 build(6) tot = tot + cnt end
print("build", tot)
