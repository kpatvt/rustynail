-- stream_closures: imitates a stream-processing helper library built from closures (each stage is a
-- function returning a next-function, no coroutines) run over generated sensor readings. small: N=1
local N = tonumber(arg and arg[1]) or 100

local seed = 5551212
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- ------------------------------------------------------------ stages: every stage returns next()
local function range(a, b, step)
  step = step or 1
  local i = a - step
  return function()
    i = i + step
    if (step > 0 and i <= b) or (step < 0 and i >= b) then return i end
  end
end

local function from_table(t)
  local i = 0
  return function()
    i = i + 1
    return t[i]
  end
end

local function map(f, src)
  return function()
    local v = src()
    if v ~= nil then return f(v) end
  end
end

local function filter(pred, src)
  return function()
    while true do
      local v = src()
      if v == nil then return nil end
      if pred(v) then return v end
    end
  end
end

local function flatmap(f, src)
  local inner
  return function()
    while true do
      if inner then
        local v = inner()
        if v ~= nil then return v end
        inner = nil
      end
      local outer = src()
      if outer == nil then return nil end
      inner = f(outer)
    end
  end
end

local function take(n, src)
  local count = 0
  return function()
    if count >= n then return nil end
    count = count + 1
    return src()
  end
end

local function take_while(pred, src)
  local done = false
  return function()
    if done then return nil end
    local v = src()
    if v == nil or not pred(v) then done = true; return nil end
    return v
  end
end

local function chunk(n, src)
  return function()
    local group = {}
    for i = 1, n do
      local v = src()
      if v == nil then break end
      group[i] = v
    end
    if #group > 0 then return group end
  end
end

local function window(n, src)
  local buf, started = {}, false
  return function()
    if not started then
      started = true
      for i = 1, n do
        local v = src()
        if v == nil then return nil end
        buf[i] = v
      end
      local out = {}
      for i = 1, n do out[i] = buf[i] end
      return out
    end
    local v = src()
    if v == nil then return nil end
    table.remove(buf, 1)
    buf[n] = v
    local out = {}
    for i = 1, n do out[i] = buf[i] end
    return out
  end
end

local function distinct(keyfn, src)
  local seen = {}
  return filter(function(v)
    local k = keyfn(v)
    if seen[k] then return false end
    seen[k] = true
    return true
  end, src)
end

local function scan(f, init, src)
  local acc = init
  return function()
    local v = src()
    if v == nil then return nil end
    acc = f(acc, v)
    return acc
  end
end

-- ------------------------------------------------------------ reductions
local function fold(f, init, src)
  local acc = init
  for v in src do acc = f(acc, v) end
  return acc
end

local function count(src) return fold(function(n) return n + 1 end, 0, src) end
local function sum(src) return fold(function(a, b) return a + b end, 0, src) end
local function collect(src)
  local t = {}
  for v in src do t[#t + 1] = v end
  return t
end
local function max_by(keyfn, src)
  local best, bk
  for v in src do
    local k = keyfn(v)
    if bk == nil or k > bk then best, bk = v, k end
  end
  return best
end

-- ------------------------------------------------------------ data source
local KINDS = { "temp", "humidity", "pressure", "light" }
local function readings(n)
  local i = 0
  local last = { 200, 500, 1000, 300 }
  return function()
    if i >= n then return nil end
    i = i + 1
    local k = rnd(4)
    last[k] = math.max(0, last[k] + rnd(41) - 21)
    return { t = i, sensor = rnd(12), kind = KINDS[k], value = last[k] }
  end
end

-- ------------------------------------------------------------ pipelines
local COUNT = N * 1000
local function avg(arr)
  local s = 0
  for i = 1, #arr do s = s + arr[i] end
  return s / #arr
end

print("total readings:", count(readings(COUNT)))

-- 1. per-kind totals
for _, kind in ipairs(KINDS) do
  local vals = function() return map(function(r) return r.value end, filter(function(r) return r.kind == kind end, readings(COUNT))) end
  local n, total = count(vals()), sum(vals())
  local mx = fold(math.max, 0, vals())
  print(string.format("%-8s n=%5d mean=%8.2f max=%5d", kind, n, total / n, mx))
end

-- 2. sliding average of temperature, count of windows above a threshold
local temps = map(function(r) return r.value end, filter(function(r) return r.kind == "temp" end, readings(COUNT)))
local smooth = map(avg, window(8, temps))
local hot = count(filter(function(a) return a > 230 end, smooth))
print("hot temperature windows:", hot)

-- 3. chunked sums: 100 readings per chunk, report a few
local chunk_sums = collect(map(function(c)
  return sum(map(function(r) return r.value end, from_table(c)))
end, chunk(100, readings(COUNT))))
print("chunks:", #chunk_sums, "first:", table.concat(chunk_sums, ",", 1, 5))
print(string.format("chunk mean %.3f", avg(chunk_sums)))

-- 4. distinct sensors in order of first appearance
local first = collect(take(12, distinct(function(r) return r.sensor end, readings(COUNT))))
local ids = {}
for i, r in ipairs(first) do ids[i] = r.sensor .. "@" .. r.t end
print("first appearances: " .. table.concat(ids, " "))

-- 5. flatmap: each reading raises zero to two alerts
local function alerts(r)
  local out = {}
  if r.kind == "temp" and r.value > 260 then out[#out + 1] = "overheat:" .. r.sensor end
  if r.kind == "pressure" and r.value > 1040 then out[#out + 1] = "highp:" .. r.sensor end
  if r.value == 0 then out[#out + 1] = "zero:" .. r.sensor end
  if r.sensor == 1 and r.kind == "light" then out[#out + 1] = "s1light" end
  return from_table(out)
end
local alert_counts, alert_total = {}, 0
for a in flatmap(alerts, readings(COUNT)) do
  local name = a:match("^(%a%w*)")
  alert_counts[name] = (alert_counts[name] or 0) + 1
  alert_total = alert_total + 1
end
local names = {}
for k in pairs(alert_counts) do names[#names + 1] = k end
table.sort(names)
local parts = {}
for _, k in ipairs(names) do parts[#parts + 1] = k .. "=" .. alert_counts[k] end
print("alerts " .. alert_total .. ": " .. table.concat(parts, " "))

-- 6. scan + take_while: running total until it passes a limit
local running = scan(function(a, r) return a + r.value end, 0, readings(COUNT))
local under = count(take_while(function(total) return total < 500000 end, running))
print("readings before running total reaches 500000:", under)

-- 7. range based pipelines
local squares = collect(take(8, filter(function(x) return x % 3 == 0 end, map(function(x) return x * x end, range(1, 1000)))))
print("squares divisible by 3: " .. table.concat(squares, " "))
print("sum of evens below 10000:", sum(filter(function(x) return x % 2 == 0 end, range(0, 9999))))
print("countdown:", table.concat(collect(range(10, 1, -3)), " "))
local pairs_sum = sum(flatmap(function(i) return map(function(j) return i * j end, range(1, i)) end, range(1, 60)))
print("triangle product sum:", pairs_sum)

-- 8. extreme reading per sensor via max_by on filtered streams
local best = {}
for s = 1, 12 do
  local r = max_by(function(x) return x.value end, filter(function(x) return x.sensor == s end, readings(COUNT)))
  best[#best + 1] = r.value .. "/" .. r.kind
end
print("max per sensor: " .. table.concat(best, " "))
