-- docs/JIT_TRACE.md section 39.2 (T3, `heatside`): exits of a full family (`maxside` side traces) that keep coming back get side traces beyond the
-- cap, up to `maxside + heatside`, within a per-state budget; exits that stop coming back stay in tier 1. Same output as the reference.

-- (1) a 40-way branch chain in one hot loop: every branch is an exit of the loop's root trace, all of them hot
local function work(n)
  local acc, t = 0, {}
  for i = 1, n do
    local k = (i * 7) % 40
    if k == 0 then acc = acc + 1
    elseif k == 1 then acc = acc + 3
    elseif k == 2 then acc = acc - 2
    elseif k == 3 then acc = acc * 1
    elseif k == 4 then acc = acc + i % 5
    elseif k == 5 then acc = acc + 7
    elseif k == 6 then t[#t + 1] = k
    elseif k == 7 then acc = acc - 1
    elseif k == 8 then acc = acc + 11
    elseif k == 9 then acc = acc + i % 3
    elseif k == 10 then acc = acc + 2
    elseif k == 11 then acc = acc - 5
    elseif k == 12 then acc = acc + 13
    elseif k == 13 then acc = acc + i % 7
    elseif k == 14 then acc = acc + 4
    elseif k == 15 then acc = acc - 3
    elseif k == 16 then acc = acc + 17
    elseif k == 17 then acc = acc + i % 11
    elseif k == 18 then acc = acc + 6
    elseif k == 19 then acc = acc - 7
    elseif k == 20 then acc = acc + 19
    elseif k == 21 then acc = acc + i % 13
    elseif k == 22 then acc = acc + 8
    elseif k == 23 then acc = acc - 9
    elseif k == 24 then acc = acc + 23
    elseif k == 25 then acc = acc + i % 17
    elseif k == 26 then acc = acc + 10
    elseif k == 27 then acc = acc - 11
    elseif k == 28 then acc = acc + 29
    elseif k == 29 then acc = acc + i % 19
    elseif k == 30 then acc = acc + 12
    elseif k == 31 then acc = acc - 13
    elseif k == 32 then acc = acc + 31
    elseif k == 33 then acc = acc + i % 23
    elseif k == 34 then acc = acc + 14
    elseif k == 35 then acc = acc - 15
    elseif k == 36 then acc = acc + 37
    elseif k == 37 then acc = acc + i % 29
    elseif k == 38 then acc = acc + 0.5
    else acc = acc - 0.25 end
  end
  return acc, #t
end
local a, b = work(60000)
print("chain", a, b)

-- (2) the same shape where the late branches stop being taken: their exits do not keep coming back
local function phases(n)
  local acc = 0
  for i = 1, n do
    local k = i < n // 4 and (i % 30) or (i % 12)
    if k == 0 then acc = acc + 1 elseif k == 1 then acc = acc + 2 elseif k == 2 then acc = acc + 3
    elseif k == 3 then acc = acc + 4 elseif k == 4 then acc = acc + 5 elseif k == 5 then acc = acc + 6
    elseif k == 6 then acc = acc + 7 elseif k == 7 then acc = acc + 8 elseif k == 8 then acc = acc + 9
    elseif k == 9 then acc = acc + 10 elseif k == 10 then acc = acc + 11 elseif k == 11 then acc = acc + 12
    elseif k == 12 then acc = acc + 13 elseif k == 13 then acc = acc + 14 elseif k == 14 then acc = acc + 15
    elseif k == 15 then acc = acc + 16 elseif k == 16 then acc = acc + 17 elseif k == 17 then acc = acc + 18
    elseif k == 18 then acc = acc + 19 elseif k == 19 then acc = acc + 20 elseif k == 20 then acc = acc + 21
    elseif k == 21 then acc = acc + 22 elseif k == 22 then acc = acc + 23 elseif k == 23 then acc = acc + 24
    elseif k == 24 then acc = acc + 25 elseif k == 25 then acc = acc + 26 elseif k == 26 then acc = acc + 27
    else acc = acc - 1 end
  end
  return acc
end
print("phases", phases(40000))

-- (3) an error raised inside one of the late branches, under pcall, and a collection in another
local function risky(n, bad)
  local acc = 0
  for i = 1, n do
    local k = i % 24
    if k == 0 then acc = acc + 1 elseif k == 1 then acc = acc + 2 elseif k == 2 then acc = acc + 3
    elseif k == 3 then acc = acc + 4 elseif k == 4 then acc = acc + 5 elseif k == 5 then acc = acc + 6
    elseif k == 6 then acc = acc + 7 elseif k == 7 then acc = acc + 8 elseif k == 8 then acc = acc + 9
    elseif k == 9 then acc = acc + 10 elseif k == 10 then acc = acc + 11 elseif k == 11 then acc = acc + 12
    elseif k == 12 then acc = acc + 13 elseif k == 13 then acc = acc + 14 elseif k == 14 then acc = acc + 15
    elseif k == 15 then acc = acc + 16 elseif k == 16 then acc = acc + 17 elseif k == 17 then acc = acc + 18
    elseif k == 18 then acc = acc + 19 elseif k == 19 then acc = acc + 20
    elseif k == 20 then if i == bad then error("bad " .. i) end acc = acc + 21
    elseif k == 21 then if i % 997 == 0 then collectgarbage("step") end acc = acc + 22
    elseif k == 22 then acc = acc + #tostring(k) else acc = acc - 1 end
  end
  return acc
end
for _, bad in ipairs({ 0, 30020, 2420, 59996 }) do
  local ok, r = pcall(risky, 60000, bad)
  print("risky", bad, ok, (tostring(r):gsub("^.-:%d+: ", "")))
end
