-- luaunit / busted style test runner: describe/it blocks, before_each hooks, assert library with deep
-- comparison and pretty-printed failure messages, pcall/error with tables, spies; runs a generated suite many times.
local M = {}

local function prettystr_sub(v, indent, seen)
  local t = type(v)
  if t == "string" then
    return string.format("%q", v)
  elseif t == "table" then
    if seen[v] then return "<cycle>" end
    seen[v] = true
    local keys = {}
    for k in pairs(v) do keys[#keys + 1] = k end
    table.sort(keys, function(a, b)
      local ta, tb = type(a), type(b)
      if ta ~= tb then return ta < tb end
      if ta == "number" or ta == "string" then return a < b end
      return tostring(a) < tostring(b)
    end)
    local parts = {}
    for _, k in ipairs(keys) do
      local ks = type(k) == "string" and k:match("^[%a_][%w_]*$") and k or "[" .. prettystr_sub(k, indent, seen) .. "]"
      parts[#parts + 1] = ks .. "=" .. prettystr_sub(v[k], indent + 1, seen)
    end
    seen[v] = nil
    return "{" .. table.concat(parts, ", ") .. "}"
  elseif t == "number" then
    if v == math.floor(v) and v > -1e15 and v < 1e15 then return string.format("%d", v) end
    return string.format("%.14g", v)
  else
    return tostring(v)
  end
end
function M.prettystr(v) return prettystr_sub(v, 0, {}) end

local function deep_equal(a, b, seen)
  if a == b then return true end
  if type(a) ~= "table" or type(b) ~= "table" then return false end
  seen = seen or {}
  if seen[a] == b then return true end
  seen[a] = b
  for k, v in pairs(a) do
    if not deep_equal(v, b[k], seen) then return false end
  end
  for k in pairs(b) do
    if a[k] == nil then return false end
  end
  return true
end

local Failure = {}
Failure.__index = Failure
Failure.__tostring = function(self) return "Failure: " .. self.msg end

local function fail(msg, level)
  error(setmetatable({ msg = msg, level = level }, Failure), (level or 1) + 1)
end

local asserts = {}
function asserts.equals(actual, expected, msg)
  if not deep_equal(actual, expected) then
    fail(string.format("%sexpected: %s, actual: %s", msg and (msg .. "\n") or "", M.prettystr(expected), M.prettystr(actual)), 2)
  end
end
function asserts.not_equals(actual, expected)
  if deep_equal(actual, expected) then fail("Received the not expected value: " .. M.prettystr(actual), 2) end
end
function asserts.is_true(v) if v ~= true then fail("expected true, got " .. M.prettystr(v), 2) end end
function asserts.is_nil(v) if v ~= nil then fail("expected nil, got " .. M.prettystr(v), 2) end end
function asserts.near(a, b, eps) if math.abs(a - b) > eps then fail(string.format("values differ beyond %g: %g %g", eps, a, b), 2) end end
function asserts.error_matches(pattern, f, ...)
  local ok, err = pcall(f, ...)
  if ok then fail("expected an error", 2) end
  local s = type(err) == "table" and tostring(err.msg) or tostring(err)
  if not s:find(pattern) then fail("error " .. M.prettystr(s) .. " does not match " .. M.prettystr(pattern), 2) end
end
function asserts.has_key(t, k) if t[k] == nil then fail("missing key " .. M.prettystr(k), 2) end end
function asserts.items_equal(a, b)
  local ca, cb = {}, {}
  for _, v in pairs(a) do ca[v] = (ca[v] or 0) + 1 end
  for _, v in pairs(b) do cb[v] = (cb[v] or 0) + 1 end
  if not deep_equal(ca, cb) then fail("items differ: " .. M.prettystr(a) .. " vs " .. M.prettystr(b), 2) end
end

-- spies
local function spy(f)
  local s = { calls = {} }
  setmetatable(s, { __call = function(self, ...)
    local args = { n = select("#", ...), ... }
    self.calls[#self.calls + 1] = args
    if f then return f(...) end
  end })
  return s
end

-- busted-style DSL
local suite
local function describe(name, fn)
  local parent = suite
  local block = { name = name, tests = {}, children = {}, before = {}, after = {}, parent = parent }
  if parent then parent.children[#parent.children + 1] = block end
  suite = block
  fn()
  suite = parent
  return block
end
local function it(name, fn) suite.tests[#suite.tests + 1] = { name = name, fn = fn } end
local function before_each(fn) suite.before[#suite.before + 1] = fn end
local function after_each(fn) suite.after[#suite.after + 1] = fn end

local function run_hooks(block, which)
  if block.parent then run_hooks(block.parent, which) end
  for _, h in ipairs(block[which]) do h() end
end

local function full_name(block, test)
  local parts = { test }
  while block do table.insert(parts, 1, block.name); block = block.parent end
  return table.concat(parts, " ")
end

local function run_block(block, results)
  for _, t in ipairs(block.tests) do
    local ok, err = xpcall(function()
      run_hooks(block, "before")
      t.fn()
      run_hooks(block, "after")
    end, function(e)
      if getmetatable(e) == Failure then return e end
      return { msg = "ERROR: " .. tostring(e):gsub("^[^:]*:%d+: ", ""), err = true }
    end)
    if ok then
      results.pass = results.pass + 1
    else
      if err.err then results.errors = results.errors + 1 else results.fail = results.fail + 1 end
      results.messages[#results.messages + 1] = full_name(block, t.name) .. ": " .. err.msg
    end
  end
  for _, c in ipairs(block.children) do run_block(c, results) end
end

-- code under test: a stack, string utils, a tiny bank account
local Stack = {}
Stack.__index = Stack
function Stack.new() return setmetatable({ n = 0 }, Stack) end
function Stack:push(...) for i = 1, select("#", ...) do self.n = self.n + 1; self[self.n] = select(i, ...) end end
function Stack:pop() if self.n == 0 then error("stack empty") end local v = self[self.n]; self[self.n] = nil; self.n = self.n - 1; return v end

local function split(s, sep)
  local out = {}
  for piece in (s .. sep):gmatch("(.-)" .. sep:gsub("%p", "%%%0")) do out[#out + 1] = piece end
  return out
end

local function build_suite(variant)
  return describe("suite" .. variant, function()
    local st
    before_each(function() st = Stack.new() end)
    describe("Stack", function()
      it("pushes and pops", function()
        st:push(1, 2, 3)
        asserts.equals(st:pop(), 3)
        asserts.equals(st.n, 2)
      end)
      it("errors when empty", function() asserts.error_matches("stack empty", st.pop, st) end)
      it("keeps a deep structure", function()
        st:push({ a = { 1, 2, { b = "c" } } })
        asserts.equals(st:pop(), { a = { 1, 2, { b = "c" } } })
      end)
      it("fails deliberately " .. variant, function()
        st:push(variant)
        asserts.equals(st:pop(), { variant, x = "y" }, "deliberate")
      end)
    end)
    describe("strings", function()
      for i = 1, 6 do
        it("splits " .. i, function()
          local s = string.rep("ab,", i) .. "end"
          local parts = split(s, ",")
          asserts.equals(#parts, i + 1)
          asserts.items_equal(parts, split(s, ","))
          asserts.not_equals(parts, {})
        end)
      end
      it("formats numbers", function()
        asserts.near(tonumber(string.format("%.3f", 1 / 3)), 0.333, 1e-9)
        asserts.equals(string.format("%5.1f|%-4d|%s", 2.25, 7, "x"), "  2.2|7   |x")
      end)
      it("raises a runtime error", function() local t = nil; return t.x end)
    end)
    describe("spies", function()
      it("records calls", function()
        local s = spy(function(a, b) return a + b end)
        for k = 1, 10 do s(k, variant) end
        asserts.equals(#s.calls, 10)
        asserts.equals(s.calls[3], { n = 2, 3, variant })
        asserts.has_key(s.calls[1], "n")
        asserts.is_nil(s.calls[11])
      end)
    end)
  end)
end

local N = tonumber(arg and arg[1]) or 12000
local totals = { pass = 0, fail = 0, errors = 0, messages = {} }
local lastmsgs
for r = 1, N do
  local res = { pass = 0, fail = 0, errors = 0, messages = {} }
  run_block(build_suite(r % 13), res)
  totals.pass = totals.pass + res.pass
  totals.fail = totals.fail + res.fail
  totals.errors = totals.errors + res.errors
  lastmsgs = res.messages
end
print(string.format("Ran %d tests: %d successes, %d failures, %d errors", totals.pass + totals.fail + totals.errors,
  totals.pass, totals.fail, totals.errors))
for _, m in ipairs(lastmsgs) do print(m) end
