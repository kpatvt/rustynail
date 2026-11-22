-- docs/JIT_TRACE.md section 39.1 (T1, `retfsel`): recursion shapes. A side trace at the RETURN of a recursive function would pop into a frame of
-- the same recursion (directly or through a partner function): the selector refuses (`retf=7` pops there). Same output as the reference.
local N = 6
local cols, d1, d2 = {}, {}, {}
local function place(i)
  local found = 0
  for j = 1, N do
    if not cols[j] and not d1[i + j] and not d2[i - j + N] then
      if i == N then
        found = found + 1
      else
        cols[j], d1[i + j], d2[i - j + N] = true, true, true
        found = found + place(i + 1)
        cols[j], d1[i + j], d2[i - j + N] = false, false, false
      end
    end
  end
  return found
end
-- (the outermost call returns into a frame with no loop around the call: the caller rule refuses that pop, only the recursion is left)
local function solve() local n = place(1) return n end
local total = 0
for r = 1, 60 do total = total + solve() end
print("solutions", total)
-- mutual recursion: a returns into b's loop, b returns into a's loop
local a, b
function a(n, acc)
  for k = 1, 3 do
    if n > 0 and (n + k) % 3 ~= 0 then acc = b(n - 1, acc + k) end
  end
  return acc
end
function b(n, acc)
  for k = 1, 2 do
    if n > 0 and (n * k) % 4 ~= 1 then acc = a(n - 1, acc + 1) end
  end
  return acc
end
local function run(n) local v = a(n, 0) return v end
local s = 0
for r = 1, 300 do s = s + run(r % 7) end
print("mutual", s)
