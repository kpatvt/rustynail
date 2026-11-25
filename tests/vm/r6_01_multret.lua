-- docs/JIT_TRACE.md section 54 (R6): native calls whose results are all wanted (`LUA_MULTRET`: the last argument or expression of a list) and
-- light C functions in tail position take the inline native call path. Same output as the reference in every mode.

local function pack(...) return { n = select("#", ...), ... } end
local function show(t)
  local o = {}
  for i = 1, t.n do o[i] = tostring(t[i]) end
  return t.n .. ":" .. table.concat(o, ",")
end

-- (1) result counts 0, 1, few, many (more than LUA_MINSTACK: the callee grows the stack), as the last argument and in constructors
local big = {}
for i = 1, 300 do big[i] = i end
for r = 1, 3 do
  print("counts", r, show(pack(string.byte("", 1, -1))), show(pack(string.byte("abc", 1, -1))), select("#", table.unpack(big)))
  local t = { string.byte("hello", 1, -1) }
  print("ctor", r, #t, t[1], t[5])
  print("nested", r, math.max(1, math.floor(r * 2.5)), string.format("%s|%s", tostring(r), tostring(nil)))
  local s = 0
  for _, v in ipairs({ table.unpack(big, 1, 50) }) do s = s + v end
  print("unpack", r, s, select(-1, table.unpack(big)))
end

-- (2) light C functions in tail position: results, errors, metamethods called from the C function, many results
local function tbyte(s) return string.byte(s, 1, -1) end
local function tsel(n, ...) return select(n, ...) end
local function tunpack(t) return table.unpack(t) end
local function terr(m) return error(m) end
local function terr2(m) return error(m, 2) end
local function tstr(x) return tostring(x) end
local function tnone() return print() end
local obj = setmetatable({}, { __tostring = function() return "OBJ" end })
local bad = setmetatable({}, { __tostring = function() error("from __tostring") end })
for r = 1, 3 do
  print("tail", r, show(pack(tbyte("xyz"))), show(pack(tsel(2, "a", "b", "c"))), select("#", tunpack(big)), tsel("#", 1, nil, 3))
  print("terr", r, pcall(terr, "boom"))
  print("terr2", r, pcall(terr2, "lvl2"))
  print("terrt", r, pcall(terr, { code = r }) == false)
  print("tstr", r, tstr(obj), pcall(tstr, bad))
  print("tnone", r, select("#", tnone()))
end

-- (3) the same with a hook set (the general path) and inside coroutines (yields from Lua code called by the C function)
local cnt = 0
debug.sethook(function() cnt = cnt + 1 end, "c")
print("hooked", show(pack(tbyte("ab"))), tsel(1, "only"), pcall(terr, "hooked error"))
debug.sethook()
print("hook calls", cnt > 0)
local co = coroutine.wrap(function(a)
  local function step(x) return coroutine.yield(x) end
  local v = step(a)
  local w = table.unpack({ v, 2 })
  local s = string.gsub("a,b", ",", function(c) return coroutine.isyieldable() and ";" or "?" end)
  return tostring(w) .. s
end)
print("co", co(1), co(10))
local co2 = coroutine.create(function() return select(2, coroutine.yield("y1")) end)
print("co2", coroutine.resume(co2))
print("co2", coroutine.resume(co2, "p", "q", "r"))
