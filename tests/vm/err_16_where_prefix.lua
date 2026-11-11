-- G2 (pcallerr): the "chunk:line: " prefix of error(), assert and library errors for every chunkname shape and level
local function try(f, ...) local ok, e = pcall(f, ...) print(ok, e) end
local names = {
  "=short", "@file.lua", "=" .. string.rep("n", 59), "=" .. string.rep("n", 60), "=" .. string.rep("n", 100),
  "@" .. string.rep("p", 59), "@" .. string.rep("p", 60), "@" .. string.rep("p", 120),
  "return 1", "line1\nline2", string.rep("s", 44), string.rep("s", 45), string.rep("s", 200), "=a\0b", "@a\0b", "",
}
for i, n in ipairs(names) do
  local f = load("local lvl = ...\nlocal x = 1\nerror('boom' .. lvl, lvl)", n)
  for lvl = 0, 3 do try(f, lvl) end
  local g = load("local function h(l) error('deep', l) end\n\nh(...)", n)
  for lvl = 0, 4 do try(g, lvl) end
  local a = load("assert(false)", n)
  try(a)
  local b = load("local t = nil\nreturn string.rep()", n)
  try(b)
  local c = load("\n\n\nreturn setmetatable(1, 2)", n)
  try(c)
end
-- stripped chunks: no line information
local d = string.dump(load("error('stripped')", "=strip"), true)
try(load(d, "=x", "b"))
local d2 = string.dump(load("local function f() error('s2', 2) end\nf()", "=strip2"), true)
try(load(d2, "=x", "b"))
-- C function at the level, numeric/string messages, level bigger than the stack
try(error, "cfun", 1)
try(error, "cfun", 2)
try(error, "cfun", 50)
try(error, "x", -1)
try(error, 12, 1)
try(error, 12.5, 2)
try(error, "m", 2^53)
try(error, "m", "2")
try(function() error(("long"):rep(100), 1) end)
try(function() error(("long"):rep(1000), 2) end)
try(function() error(("x"):rep(272), 1) end)
try(function() error(("x"):rep(273), 1) end)
try(function() error(("x"):rep(150), 1) end)
-- very long chunkname and big line number
local big = load(string.rep("\n", 99999) .. "error('big')", "=" .. string.rep("z", 70))
try(big)
-- inside coroutines, xpcall handlers, __close, __gc
local co = coroutine.wrap(function() local ok, e = pcall(error, "in co", 1); print(ok, e); error("co err", 1) end)
print(pcall(co))
print(xpcall(function() error("xp") end, function(m) return "H:" .. m end))
print(xpcall(function() error("xp", 2) end, function(m) return "H:" .. m end))
do
  local ok, e = pcall(function()
    local x <close> = setmetatable({}, {__close = function() error("in close") end})
    error("body")
  end)
  print(ok, e)
end
print(pcall(string.rep))
print(pcall(setmetatable, 1))
print(pcall(function() return ("x"):bad() end))
print(select(2, pcall(function() local t = setmetatable({}, {__index = function(t, k) error("idx " .. k) end}); return t.foo end)))
-- recursion: stack overflow messages keep the prefix
local function rec() return 1 + rec() end
local ok, e = pcall(rec)
print(ok, (tostring(e):gsub("%d+", "N")))
local function rec2() error("rec2") ; end
for i = 1, 200 do pcall(rec2) end
print(pcall(rec2))
-- error in a finalizer is a warning, not an error; the message is built with the same prefix
setmetatable({}, {__gc = function() error("in gc") end})
collectgarbage()
print("done")
