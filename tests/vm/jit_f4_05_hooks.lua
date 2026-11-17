-- F4: locals vs temporaries seen through hooks: debug.getlocal of the named locals must be exact
local log = {}
local function snap(level)
  local out = {}
  local i = 1
  while true do
    local name, val = debug.getlocal(level, i)
    if not name then break end
    if name:sub(1, 1) ~= "(" then out[#out + 1] = name .. "=" .. tostring(val) end
    i = i + 1
  end
  return table.concat(out, ",")
end
local function work(n)
  local s = 0.0
  local k = 0
  for i = 1, n do
    local a = i * 1.5
    local b = a * 2 + 0.25
    k = k + 1
    s = s + (a * b + (i + 0.5) * (a - b)) * 0.125
    if k == 40 then
      debug.sethook(function()
        log[#log + 1] = snap(3)
        if #log >= 12 then debug.sethook() end
      end, "", 1)
    end
  end
  return s
end
local r = 0.0
for k = 1, 8 do
  log = {}
  r = r + work(120)
end
print(string.format("%.9g", r), #log)
for i = 1, #log do print(log[i]) end
-- the named locals right after a side exit in the middle of an iteration: dead locals are still visible (never pruned)
local seen = {}
local function probe(t, u, n, at)
  local s = 0.0
  for i = 1, n do
    local a = t[i] * 2.5
    local b = a + 0.5
    local c = b * 1.5 - a
    s = s + c * (u[i] + 0.25)
    if i == at then
      local names = {}
      for k = 1, 16 do
        local name, v = debug.getlocal(1, k)
        if name == "a" or name == "b" or name == "c" or name == "s" or name == "i" then
          names[#names + 1] = name .. "=" .. tostring(v)
        end
      end
      seen[#seen + 1] = table.concat(names, ",")
    end
  end
  return s
end
local t, u = {}, {}
for i = 1, 60 do t[i] = i * 0.5; u[i] = i * 0.125 end
for k = 1, 30 do probe(t, u, 60, 0) end
print(string.format("%.9g", probe(t, u, 60, 33))) -- the guard of `i == at` fails: an exit after `a`, `b`, `c` died
u[45] = 3 -- an integer where the trace expects a float: exit in the middle of the iteration
t[45] = 7
print(string.format("%.9g", probe(t, u, 60, 45)))
u[45] = "2.5"
print(string.format("%.9g", probe(t, u, 60, 45)))
for i = 1, #seen do print(seen[i]) end
