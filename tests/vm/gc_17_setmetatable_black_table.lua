-- setmetatable on a black (old / already traversed) table with a fresh white metatable: the collector barrier must keep the metatable
-- alive (the metatable is referenced only by the table). The loops are hot enough for the JIT's inline setmetatable store.
local N = 300

local function olds(n)
  local t = {}
  for i = 1, n do t[i] = {} end
  return t
end

-- a throw-away use caches "no __gc" in each metatable (the inline store needs the cached bit)
local function pre(n)
  local mts = {}
  for i = 1, n do
    local m = {__index = {v = i}}
    setmetatable({}, m)
    mts[i] = m
  end
  return mts
end

local function apply(ts, mts, n)
  for i = 1, n do
    local mt = mts[i]
    mts[i] = false
    setmetatable(ts[i], mt)
  end
end

local function check(ts, n)
  local s = 0
  for i = 1, n do s = s + ts[i].v end
  return s
end

-- generational: the tables are old after two full collections, the metatables are young
collectgarbage("generational")
local total = 0
for r = 1, 30 do
  local ts = olds(N)
  collectgarbage() collectgarbage()
  local mts = pre(N)
  apply(ts, mts, N)
  collectgarbage("step", 0)
  local junk = {}
  for i = 1, 2000 do junk[i % 50 + 1] = {i} end
  collectgarbage("step", 0)
  total = total + check(ts, N)
end
print("gen", total)

-- incremental: the smallest steps (one unit of work each) leave the globals table's elements black part-way through the mark phase;
-- the metatables are created (white) after that and finish the cycle with the table as their only referrer
collectgarbage("incremental")
local M = 3000
total = 0
for r = 1, 60 do
  G = nil
  collectgarbage() collectgarbage()
  collectgarbage("stop")
  G = olds(M)
  collectgarbage("incremental", 100, 1, 0)
  for k = 1, r * 40 do collectgarbage("step", 0) end
  collectgarbage("incremental", 100, 100, 13)
  local mts = pre(M)
  apply(G, mts, M)
  mts = nil
  repeat until collectgarbage("step", 0)
  repeat until collectgarbage("step", 0)
  local junk = {}
  for i = 1, 2000 do junk[i % 50 + 1] = {i} end
  total = total + check(G, M)
end
print("inc", total)
G = nil
