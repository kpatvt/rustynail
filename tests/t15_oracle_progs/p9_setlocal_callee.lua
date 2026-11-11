local function poke(name, v)
  for i = 1, 30 do
    local nm = debug.getlocal(2, i)
    if nm == name then debug.setlocal(2, i, v) return end
  end
end
local function f(n, v)
  local a, b = 1, 2.5
  local s = 0
  for i = 1, n do
    s = s + a
    if i == 11 then poke("a", v) end
    s = s + a
    if type(a) ~= "number" then a = 1 end
    b = b + 1
  end
  return s, b
end
print(f(30, 5), f(30, 7.5))
print(pcall(f, 30, "x"))
print(pcall(f, 30, {}))
print(pcall(f, 30, nil))
