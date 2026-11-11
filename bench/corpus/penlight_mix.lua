-- A Penlight-style mix: pl.stringx (split/strip/replace/expandtabs/startswith), pl.tablex (map/filter/reduce/keys/
-- deepcompare/zip), pl.List with method chaining and __concat/__eq/__tostring, pl.utils (bind1, memoize, varargs),
-- pl.func-ish compose. Workload: a log analysis pipeline.
local unpack = table.unpack or unpack
local utils = {}
function utils.pack(...) return { n = select("#", ...), ... } end
function utils.bind1(fn, p) return function(...) return fn(p, ...) end end
function utils.bind2(fn, p) return function(x, ...) return fn(x, p, ...) end end
function utils.memoize(func)
  local cache = {}
  return function(k)
    local res = cache[k]
    if res == nil then
      res = func(k)
      cache[k] = res
    end
    return res
  end
end
function utils.compose(...)
  local fns = { ... }
  return function(...)
    local args = utils.pack(...)
    for i = #fns, 1, -1 do args = utils.pack(fns[i](unpack(args, 1, args.n))) end
    return unpack(args, 1, args.n)
  end
end
function utils.choose(cond, a, b) if cond then return a else return b end end

-- stringx
local stringx = {}
function stringx.split(s, re, n)
  local res = {}
  local plain = true
  if not re then re = "%s+"; plain = false end
  local i1, ls = 1, #s
  while true do
    local i2, i3 = s:find(re, i1, plain)
    if not i2 or (n and #res >= n - 1) then
      local last = s:sub(i1)
      if last ~= "" or not (re == "%s+") then res[#res + 1] = last end
      break
    end
    if i2 > i1 or re ~= "%s+" then res[#res + 1] = s:sub(i1, i2 - 1) end
    i1 = i3 + 1
    if i1 > ls + 1 then break end
  end
  return res
end
function stringx.strip(s, chrs)
  if not chrs then return (s:gsub("^%s+", ""):gsub("%s+$", "")) end
  local cs = "[" .. chrs:gsub("[%^%]%-%%]", "%%%0") .. "]"
  return (s:gsub("^" .. cs .. "+", ""):gsub(cs .. "+$", ""))
end
function stringx.startswith(s, prefix) return s:find(prefix, 1, true) == 1 end
function stringx.endswith(s, suffix) return #s >= #suffix and s:find(suffix, #s - #suffix + 1, true) and true or false end
function stringx.replace(s, old, new, n)
  return (s:gsub(old:gsub("[%-%.%+%[%]%(%)%$%^%%%?%*]", "%%%0"), (new:gsub("%%", "%%%%")), n))
end
function stringx.expandtabs(s, tabsize)
  tabsize = tabsize or 8
  return (s:gsub("([^\t\r\n]*)\t", function(before)
    return before .. (" "):rep(tabsize - #before % tabsize)
  end))
end
function stringx.count(s, sub)
  local n, i = 0, 1
  while true do
    local a, b = s:find(sub, i, true)
    if not a then return n end
    n = n + 1
    i = b + 1
  end
end
function stringx.join(sep, list) return table.concat(list, sep) end

-- tablex
local tablex = {}
function tablex.map(fun, t, ...)
  local res = {}
  for k, v in pairs(t) do res[k] = fun(v, ...) end
  return setmetatable(res, getmetatable(t))
end
function tablex.imap(fun, t, ...)
  local res = {}
  for i = 1, #t do res[i] = fun(t[i], ...) or false end
  return res
end
function tablex.filter(t, pred, arg)
  local res, k = {}, 1
  for i = 1, #t do
    local v = t[i]
    if pred(v, arg) then res[k] = v; k = k + 1 end
  end
  return res
end
function tablex.reduce(fun, t, memo)
  local n = #t
  if n == 0 then return memo end
  local res = memo and fun(memo, t[1]) or t[1]
  for i = 2, n do res = fun(res, t[i]) end
  return res
end
function tablex.keys(t)
  local res = {}
  for k in pairs(t) do res[#res + 1] = k end
  table.sort(res, function(a, b) return tostring(a) < tostring(b) end)
  return res
end
function tablex.deepcompare(t1, t2)
  local ty1, ty2 = type(t1), type(t2)
  if ty1 ~= ty2 then return false end
  if ty1 ~= "table" then return t1 == t2 end
  for k1, v1 in pairs(t1) do
    local v2 = t2[k1]
    if v2 == nil or not tablex.deepcompare(v1, v2) then return false end
  end
  for k2 in pairs(t2) do if t1[k2] == nil then return false end end
  return true
end
function tablex.zip(...)
  local args = { ... }
  local res = {}
  for i = 1, #args[1] do
    local row = {}
    for j = 1, #args do row[j] = args[j][i] end
    res[i] = row
  end
  return res
end
function tablex.count_map(t)
  local res = {}
  for _, v in ipairs(t) do res[v] = (res[v] or 0) + 1 end
  return res
end

-- List
local List = {}
List.__index = List
setmetatable(List, { __call = function(cls, t) return cls.new(t) end })
function List.new(t) return setmetatable(t or {}, List) end
function List:append(v) self[#self + 1] = v; return self end
function List:extend(l) for i = 1, #l do self[#self + 1] = l[i] end; return self end
function List:map(fun, ...) return List.new(tablex.imap(fun, self, ...)) end
function List:filter(pred, arg) return List.new(tablex.filter(self, pred, arg)) end
function List:reduce(fun, memo) return tablex.reduce(fun, self, memo) end
function List:slice(i, j)
  local res = List.new()
  for k = i or 1, j or #self do res[#res + 1] = self[k] end
  return res
end
function List:sort(cmp) table.sort(self, cmp); return self end
function List:join(sep) return table.concat(tablex.imap(tostring, self), sep or "") end
function List:contains(x) for i = 1, #self do if self[i] == x then return true end end return false end
function List:partition(fun)
  local res = {}
  for i = 1, #self do
    local k = fun(self[i])
    if k == nil then k = "<nil>" end
    if not res[k] then res[k] = List.new() end
    res[k]:append(self[i])
  end
  return res
end
function List.__concat(a, b) return List.new():extend(a):extend(b) end
function List.__eq(a, b)
  if #a ~= #b then return false end
  for i = 1, #a do if a[i] ~= b[i] then return false end end
  return true
end
function List:__tostring() return "{" .. self:join(",") .. "}" end

-- Workload: synthesize log lines and analyze them
local levels = { "INFO", "WARN", "ERROR", "DEBUG" }
local paths = { "/api/users", "/api/items", "/static/app.js", "/login", "/api/orders" }
local function make_lines(n, seed)
  local out = {}
  for i = 1, n do
    seed = (seed * 16807) % 2147483647
    local lvl = levels[1 + seed % 4]
    local p = paths[1 + math.floor(seed / 7) % 5]
    out[i] = string.format("  2026-11-%02d\t%s\t%s  status=%d\tms=%d  user=u%d  ", 1 + i % 28, lvl, p,
      200 + (seed % 3) * 100 + (seed % 5), seed % 997, seed % 37)
  end
  return out
end

local N = tonumber(arg and arg[1]) or 400
local parse_kv = utils.memoize(function(field)
  local k, v = field:match("^(%w+)=(%w+)$")
  return { k = k, v = tonumber(v) or v }
end)
local upper = utils.compose(string.upper, stringx.strip)
local total_ms, errors, warn_paths, report = 0, 0, 0, nil
for r = 1, N do
  local lines = List(make_lines(400, r))
  local recs = lines:map(function(line)
    local f = stringx.split(stringx.expandtabs(stringx.strip(line), 4))
    local rec = { date = f[1], level = f[2], path = f[3] }
    for i = 4, #f do
      local kv = parse_kv(f[i])
      rec[kv.k] = kv.v
    end
    return rec
  end)
  local errs = recs:filter(function(rec) return rec.level == "ERROR" end)
  errors = errors + #errs
  total_ms = total_ms + recs:map(function(rec) return rec.ms end):reduce(function(a, b) return a + b end)
  local by_path = recs:partition(function(rec) return rec.path end)
  for _, k in ipairs(tablex.keys(by_path)) do
    if stringx.startswith(k, "/api") then
      warn_paths = warn_paths + #by_path[k]:filter(function(rec) return rec.level == "WARN" end)
    end
  end
  local counts = tablex.count_map(tablex.imap(function(rec) return rec.status end, recs))
  local zipped = tablex.zip(lines:slice(1, 5), recs:slice(1, 5))
  local a = List { 1, 2, 3 } .. List { 4, 5 }
  if a == List { 1, 2, 3, 4, 5 } and tablex.deepcompare(zipped[1][2], recs[1]) then total_ms = total_ms + 1 end
  local keys = tablex.keys(counts)
  report = upper("  " .. stringx.join(",", keys) .. "  ") .. " " .. stringx.count(lines:join("|"), "ERROR")
    .. " " .. stringx.replace(lines[1], "\t", "->") .. tostring(a) .. tostring(stringx.endswith(lines[2], "  "))
end
print("errors", errors, "ms", total_ms, "warn_api", warn_paths)
print(report)
