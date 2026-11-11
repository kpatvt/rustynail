-- Vararg-heavy utilities: printf-like formatters, curry/partial/compose, pack/unpack,
-- select('#') and select(i, ...) loops, argument validation, forwarding through layers. small: N=3
local N = tonumber(arg and arg[1]) or 400000

local unpack = table.unpack or unpack
local pack = table.pack or function(...) return { n = select('#', ...), ... } end

local M = {}

-- Argument validation --------------------------------------------------------------------------
function M.checkTypes(name, spec, ...)
  local n = select('#', ...)
  for i = 1, #spec do
    local v = select(i, ...)
    if type(v) ~= spec[i] then
      return false, string.format("%s: arg #%d expected %s, got %s", name, i, spec[i], type(v))
    end
  end
  return true, n
end

function M.count(...) return select('#', ...) end

function M.nilCount(...)
  local c = 0
  for i = 1, select('#', ...) do
    if select(i, ...) == nil then c = c + 1 end
  end
  return c
end

-- Reductions over varargs ------------------------------------------------------------------------
function M.sum(...)
  local s = 0
  for i = 1, select('#', ...) do s = s + (select(i, ...)) end
  return s
end

function M.max(first, ...)
  local best = first
  local args = { ... }
  for i = 1, #args do
    if args[i] > best then best = args[i] end
  end
  return best
end

function M.sumTail(first, ...)
  if select('#', ...) == 0 then return first end
  return first + M.sumTail(...)
end

function M.reverse(...)
  local n = select('#', ...)
  if n <= 1 then return ... end
  local args = pack(...)
  local out = {}
  for i = 1, n do out[i] = args[n - i + 1] end
  return unpack(out, 1, n)
end

function M.concatAll(sep, ...)
  local parts = {}
  for i = 1, select('#', ...) do
    parts[#parts + 1] = tostring((select(i, ...)))
  end
  return table.concat(parts, sep)
end

-- printf-like formatting ----------------------------------------------------------------------------
function M.sprintf(fmt, ...)
  local args = pack(...)
  local i = 0
  return (fmt:gsub("%%(%a)", function(spec)
    i = i + 1
    if i > args.n then return "<missing>" end
    local v = args[i]
    if spec == "d" then return string.format("%d", v)
    elseif spec == "s" then return tostring(v)
    elseif spec == "q" then return string.format("%q", tostring(v))
    elseif spec == "f" then return string.format("%.2f", v)
    end
    return "?"
  end))
end

local logLines = 0
function M.log(level, fmt, ...)
  local line = string.format("[%s] ", level) .. M.sprintf(fmt, ...)
  logLines = logLines + #line
  return line
end

function M.logger(level)
  return function(...) return M.log(level, ...) end
end

-- Function combinators -----------------------------------------------------------------------------------
function M.partial(f, ...)
  local bound = pack(...)
  return function(...)
    local extra = pack(...)
    local all = {}
    for i = 1, bound.n do all[i] = bound[i] end
    for i = 1, extra.n do all[bound.n + i] = extra[i] end
    return f(unpack(all, 1, bound.n + extra.n))
  end
end

function M.curry(f, arity)
  arity = arity or 2
  local function collect(args, n)
    return function(...)
      local m = select('#', ...)
      local nargs = { unpack(args, 1, n) }
      for i = 1, m do nargs[n + i] = (select(i, ...)) end
      if n + m >= arity then return f(unpack(nargs, 1, n + m)) end
      return collect(nargs, n + m)
    end
  end
  return collect({}, 0)
end

function M.compose(...)
  local fns = pack(...)
  return function(...)
    local res = pack(...)
    for i = fns.n, 1, -1 do
      res = pack(fns[i](unpack(res, 1, res.n)))
    end
    return unpack(res, 1, res.n)
  end
end

function M.memoize1(f)
  local cache = {}
  return function(x)
    local v = cache[x]
    if v == nil then v = f(x); cache[x] = v end
    return v
  end
end

function M.tap(f, ...)
  local n = select('#', ...)
  local args = { ... }
  f(...)
  return unpack(args, 1, n)
end

-- Forwarding layers: each layer adds bookkeeping and passes everything on ---------------------------------
local layerCalls = 0
local function layer3(...) layerCalls = layerCalls + 1; return select('#', ...), ... end
local function layer2(a, ...) return layer3(a, ...) end
local function layer1(...) return layer2(...) end
local function wrapper(f) return function(...) return f(...) end end
local wrapped = wrapper(wrapper(wrapper(layer1)))

-- Driver ------------------------------------------------------------------------------------------------------
local ok, err = M.checkTypes("demo", { "number", "string" }, 1, 2)
print(ok, err)
print(M.checkTypes("demo", { "number", "string" }, 1, "x"))
print(M.count(), M.count(nil), M.count(nil, nil), M.count(1, nil, 3), M.nilCount(nil, 1, nil, nil))
print(M.sprintf("%s has %d items costing %f (%q)", "cart", 3, 9.5, "hi"), M.sprintf("%d %d", 1))
print(M.concatAll("-", M.reverse(1, 2, 3, "four", true)))

local add3 = M.curry(function(a, b, c) return a + b + c end, 3)
print(add3(1)(2)(3), add3(1, 2)(3), add3(1)(2, 3), add3(1, 2, 3))
local inc = M.partial(function(a, b) return a + b end, 1)
local scaleThenInc = M.compose(inc, function(x, y) return x * y end)
print(scaleThenInc(6, 7), M.sum(unpack({ 1, 2, 3, 4, 5 })), M.sumTail(1, 2, 3, 4), M.max(3, 9, 2))
print(wrapped("a", nil, "c", nil))

local info = M.logger("info")
local acc, checksum = 0, 0
for i = 1, N do
  local n = i % 7
  local args = {}
  for k = 1, n do args[k] = i * k end
  acc = acc + M.sum(unpack(args, 1, n))
  acc = acc + M.max(i, unpack(args, 1, n))
  local cnt = select(1, wrapped(unpack(args, 1, n)))
  checksum = (checksum + cnt * i) % 1000003
  if i % 5 == 0 then
    local line = info("%d of %d: %s", i, N, M.concatAll(",", unpack(args, 1, n)))
    checksum = (checksum + #line) % 1000003
  end
  if i % 11 == 0 then
    local f = M.curry(function(a, b, c, d) return a * b + c - d end, 4)
    acc = acc + f(i)(2)(3, 4)
  end
  if i % 13 == 0 then
    local c = M.compose(M.partial(M.sum, 1, 2), M.reverse, M.tap)
    acc = acc + c(M.count, 1, 2)
  end
end

print(string.format("acc=%d checksum=%d layerCalls=%d logChars=%d", acc, checksum, layerCalls, logLines))
print(M.log("done", "%s %d %f", "finished", N, 3.14159))

local memo = M.memoize1(function(n) layerCalls = layerCalls + 1; return n * n end)
local ms = 0
for i = 1, N do ms = ms + memo(i % 100) end
print(string.format("memo sum=%d calls=%d", ms, layerCalls))
