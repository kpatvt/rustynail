-- docs/JIT_TRACE.md section 43.1 (TR24c, `restarts`): a root recording whose iteration ends with a slot of another type than its first read checked restarts in
-- place from the new types. Same output as the reference in every mode.

-- (1) an accumulator that starts as an integer and becomes a float in the first iteration
local function acc(n)
  local a = 0
  for i = 1, n do a = a + 0.5 end
  return a
end
local t = 0
for r = 1, 40 do t = t + acc(300) end
print("acc", t)

-- (2) a slot that is nil in the first iteration and a table afterwards (the `ipairs` loop with a late `who`)
local function walk(list)
  local who
  local n = 0
  for _, v in ipairs(list) do
    if who then n = n + who.x end
    who = v
  end
  return n
end
local list = {}
for i = 1, 200 do list[i] = { x = i } end
local w = 0
for r = 1, 60 do w = w + walk(list) end
print("walk", w)

-- (3) a type that flips in every iteration (the restart budget is spent, the head is re-armed or blacklisted)
local function flip(n)
  local a = true
  local c = 0
  for i = 1, n do
    a = not a
    if a then c = c + 1 end
  end
  return c
end
local f = 0
for r = 1, 40 do f = f + flip(300) end
print("flip", f)

-- (4) a string / number slot, and an error raised in a later iteration under pcall
local function mix(n, bad)
  local s = 0
  local v = "1"
  for i = 1, n do
    s = s + tonumber(v)
    v = i
    if bad and i == n - 3 then error("boom " .. i) end
  end
  return s
end
local m = 0
for r = 1, 30 do m = m + mix(200, false) end
print("mix", m)
for r = 1, 5 do print("mixerr", pcall(mix, 200, true)) end

-- (5) allocation in the loop with a collection in the middle
local function mk(n)
  local last
  local cnt = 0
  for i = 1, n do
    if last then cnt = cnt + #last end
    last = { i, i + 1 }
    if i % 97 == 0 then collectgarbage() end
  end
  return cnt
end
local k = 0
for r = 1, 30 do k = k + mk(300) end
print("mk", k)
