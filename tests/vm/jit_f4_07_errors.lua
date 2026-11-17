-- F4: errors inside expressions with pcall around; the frame's temporaries are dead after the error, locals survive
local function f(a, b, n)
  local s = 0.0
  local done = 0
  for i = 1, n do
    s = s + (a[i] * 2.5 + b[i] * 1.5) * (a[i] - 0.5)
    done = i
  end
  return s, done
end
local a, b = {}, {}
for i = 1, 100 do a[i] = i * 0.5; b[i] = i * 0.25 end
local r = 0.0
for k = 1, 30 do r = r + (f(a, b, 100)) end
print(string.format("%.9g", r))
b[70] = {}
print(pcall(f, a, b, 100))
b[70] = 1
a[80] = "x"
print(pcall(f, a, b, 100))
a[80] = nil
print(pcall(f, a, b, 100))
a[80] = 4.5
local function g(n)
  local t = setmetatable({}, { __index = function(_, k) if k == 33 then error("boom " .. k) end return k * 0.5 end })
  local s = 0.0
  for i = 1, n do s = s + (t[i] * 1.5 + i) * (t[i] + 0.25) end
  return s
end
for k = 1, 20 do g(30) end
print(g(32))
print(pcall(g, 40))
print(select(2, xpcall(g, function(m) return "H:" .. m end, 40)))
