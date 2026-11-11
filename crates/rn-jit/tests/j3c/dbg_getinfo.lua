local function desc(l, what)
  local i = debug.getinfo(l, what or "nSltufrL")
  if not i then return "nil" end
  local ls = {}
  if i.activelines then for k in pairs(i.activelines) do ls[#ls + 1] = k end table.sort(ls) end
  return table.concat({tostring(i.name), tostring(i.namewhat), i.what, i.short_src, i.linedefined,
    i.lastlinedefined, i.currentline, i.nups, i.nparams, tostring(i.isvararg), tostring(i.istailcall),
    i.ftransfer, i.ntransfer, table.concat(ls, ",")}, "|")
end
local function leaf(l) local r = desc(l) return r end
local function tc(l) return desc(l) end
local function va(l, ...) local r = leaf(l) return r end
local obj = {}
function obj:meth(l) local r = tc(l) return r end
function gfun(l) local r = obj:meth(l) return r end
local function mid(l)
  local r = va(l, 1, 2, 3)
  local r2 = gfun(l)
  return r .. "#" .. r2
end
local function top(n, l)
  local acc = {}
  for i = 1, n do acc[#acc + 1] = mid(l) end
  return acc
end
for l = 1, 9 do
  local acc = top(4, l)
  P("level", l, acc[1] == acc[4], acc[1])
  P("level", l, acc[2])
end
P(debug.getinfo(100, "n"))
P(pcall(debug.getinfo, 1, "X"))
P(pcall(debug.getinfo, 1, ">"))
-- function-argument forms
local function f1(a, b, ...) return a end
local i = debug.getinfo(f1, "SuL")
P(i.what, i.linedefined, i.lastlinedefined, i.nparams, i.isvararg, i.nups)
i = debug.getinfo(print, "Su")
P(i.what, i.short_src, i.linedefined, i.nups, i.nparams, i.isvararg)
P(debug.getinfo(f1, "f").func == f1, debug.getinfo(1, "f").func ~= nil)
-- currentline in a hot loop
local lines = {}
for i = 1, 60 do
  local x = i * 2
  lines[#lines + 1] = debug.getinfo(1, "l").currentline
  if i % 3 == 0 then
    lines[#lines + 1] = debug.getinfo(1, "l").currentline
  elseif i % 5 == 0 then
    local y = x + 1
    lines[#lines + 1] = debug.getinfo(1, "l").currentline
  end
end
P(table.concat(lines, ","))
-- currentline at the exits of various instruction classes, seen from a traceback in an xpcall handler
local function tb(m) return (debug.traceback(tostring(m), 1):gsub("\n%s*%[C%]: in %?$", "")) end
local errs = {
  function(t) return t.x.y end,
  function(t) return t + 1 end,
  function(t) return t() end,
  function(t) return t < 1 end,
  function(t) return "a" .. t end,
  function(t) for i = t, 3 do end end,
  function(t) return #t.x end,
  function(t) local a <close> = t.x return 1 end,
  function(t) return -{} end,
  function(t) t.x.y = 1 end,
  function(t) return 1 // (t.n or 0) end,
  function(t) return math.maxinteger < "x" end,
}
for k, f in ipairs(errs) do
  for rep = 1, 3 do
    local ok, m = xpcall(f, tb, {})
    if rep == 3 then P("err", k, ok, m) end
  end
end
return table.concat(out, "\n")
