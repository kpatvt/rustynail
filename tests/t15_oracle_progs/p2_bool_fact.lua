-- review finding 2: a non-singleton (Bool) fact is never guarded by the stub, a callee clears the escaped local.
local n = 300
local flag = false
local c = 0
local function clr() flag = nil end
for i = 1, n do
  if i % 2 == 0 then flag = true else flag = false end
  if i % 51 == 0 then clr() end
  if flag then c = c + 1 end
end
print(c)

-- the same with a Num fact and debug.setlocal from a callee
local function setx(v)
  for i = 1, 20 do
    local nm = debug.getlocal(2, i)
    if nm == "x" then debug.setlocal(2, i, v) return end
  end
end
local function g(n, v)
  local x = 1
  local s = 0
  for i = 1, n do
    if i % 2 == 0 then x = 1 else x = 2.5 end
    if i == 7 then setx(v) end
    if x == 1 then s = s + 1 else s = s + 2 end
  end
  return s
end
print(g(20, "str"), g(20, nil), g(20, true), g(20, {}))
local function h(n, v)
  local x = 1
  local s = 0
  for i = 1, n do
    if i % 2 == 0 then x = 1 else x = 2.5 end
    if i == 7 then setx(v) end
    s = s + x
  end
  return s
end
print(pcall(h, 20, "str"))
print(pcall(h, 20, 3))
print(pcall(h, 20, nil))
