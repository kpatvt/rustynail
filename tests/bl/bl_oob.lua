-- BUNDLE-L (`aidx`, `bhoist`): array loops whose index range ends exactly at, one past, or far past the array part, with stores and loads, offsets
-- k + c on both sides, wrap-around counters, shrinking / growing arrays, two and three dimensions. Output must equal the reference interpreter's.
local H = 0
local function mix(x) H = (H * 31 + (math.tointeger(x) or math.floor(x * 8))) % 1000000007 end

-- loads of a[k + c] for lim around the array size
local function ld(a, lim, c)
  local s = 0
  for k = 1, lim do s = s + (a[k + c] or -1) end
  return s
end
local function mk(n) local a = {} for i = 1, n do a[i] = i end return a end
for r = 1, 40 do ld(mk(16), 16, 0) ld(mk(16), 15, 1) end
for _, n in ipairs({ 16, 15, 17 }) do
  for _, lim in ipairs({ 0, 1, 14, 15, 16, 17, 18, 40 }) do
    for c = -2, 2 do mix(ld(mk(n), lim, c)) end
  end
end
print("ld", H)

-- stores that run to the end of the array part and past it (the table grows or the key goes to the hash)
local function st(a, lim, c)
  for k = 1, lim do a[k + c] = k * 2 end
  return #a
end
for r = 1, 40 do st(mk(16), 16, 0) end
for _, lim in ipairs({ 3, 15, 16, 17, 30 }) do
  for c = -1, 2 do
    local a = mk(16)
    mix(st(a, lim, c))
    for i = 1, 34 do mix(a[i] or -7) end
  end
end
print("st", H)

-- float elements, a different element type at the end (the tag guard), nil holes
local function fl(a, lim)
  local s = 0.0
  for k = 1, lim do s = s + a[k] end
  return s
end
local f = {} for i = 1, 20 do f[i] = i + 0.5 end
for r = 1, 40 do fl(f, 20) end
f[20] = 7 mix(fl(f, 20))
f[20] = 1.5 f[10] = 1 mix(fl(f, 20))
print("fl", H, pcall(fl, f, 21))
f[10] = nil print("hole", pcall(fl, f, 20))

-- a counter that is not a numeric `for` (wraps), broken by another exit
local function wh(a, n)
  local k, s = 1, 0
  while n ~= 0 do
    s = s + (a[k] or 0)
    n = n - 1
    k = k + 1
    if k > 50 then break end
  end
  return s
end
local w = mk(20)
for r = 1, 40 do wh(w, 30) end
mix(wh(w, 30)) mix(wh(w, -3)) mix(wh(w, 0)) mix(wh(w, 19)) mix(wh(w, 20))
print("while", H)

-- loop bounds at the integer limits
local function lim_loop(a)
  local s = 0
  for k = math.maxinteger - 2, math.maxinteger do s = s + (a[k] or 1) end
  for k = math.mininteger, math.mininteger + 2 do s = s + (a[k] or 2) end
  for k = -3, 3 do s = s + (a[k] or 3) end
  for k = 0, 2 do s = s + (a[k] or 4) end
  return s
end
for r = 1, 40 do lim_loop(w) end
mix(lim_loop(w))
print("limits", H)

-- table.insert / remove / a metamethod change the size inside the loop; the array is replaced inside the loop
local function dyn(a, lim)
  local s = 0
  for k = 1, lim do
    s = s + (a[k] or 0)
    if k == 5 then a[#a + 1] = 100 end
    if k == 6 then table.remove(a, 1) end
    if k == 8 then a = mk(3) end
  end
  return s
end
for r = 1, 40 do dyn(mk(12), 12) end
mix(dyn(mk(12), 12)) mix(dyn(mk(7), 14)) mix(dyn(mk(30), 40))
print("dyn", H)

-- matrices: the row is loaded in the loop (its size is not loop invariant), rows of different lengths
local function mm(a, b, n)
  local s = 0
  for i = 1, n do
    local ai = a[i]
    for j = 1, n do
      for k = 1, n do s = s + ai[k] * b[k][j] end
    end
  end
  return s
end
local function mat(n, m) local t = {} for i = 1, n do local r = {} for j = 1, m do r[j] = (i + j) % 5 end t[i] = r end return t end
for r = 1, 10 do mm(mat(6, 6), mat(6, 6), 6) end
mix(mm(mat(6, 6), mat(6, 6), 6))
local rag = mat(6, 6) rag[4] = { 1, 2, 3 }
print("mat", H, pcall(mm, mat(6, 6), rag, 6))
print("matb", pcall(mm, mat(6, 6), mat(6, 6), 7))
-- jit-diff bl seed 4 (aidx): a loop-carried index used through a pre-roll `k + 1` whose operand must not be split in the body copy
local function sd4(n)
  local a, b = {}, {}
  for i = 1, 21 do a[i] = (i * 3) % 11 + 0.5; b[i] = i % 7 end
  local h = 0
  for r = 1, n do
    local s = 0.0
    for k = 1, 58 do s = s + (a[k] or 1) * (b[k + 1] or 2) end
    h = (h * 31 + math.floor(s)) % 1000003
    if r % 3 == 0 then a[#a + 1] = r; b[#b + 1] = r % 5 end
  end
  return h
end
print("sd4", sd4(55))
print("final", H)
