-- tostring / %s of every kind of value (addresses normalised): luaL_tolstring fast paths
local function norm(s) return (tostring(s):gsub("0x%x+", "0xADDR")) end
local function try(f, ...) local ok, r = pcall(f, ...) print(ok, norm(r)) end
local co = coroutine.create(function() end)
local vals = {{}, print, function() end, co, io.stdout, nil, true, false, 1, -1, 1.5, -0.0, 0/0, math.huge, -math.huge, 2^63, math.mininteger, "str", ""}
for i = 1, 19 do
  local v = vals[i]
  print(i, norm(tostring(v)), norm(string.format("%s", v)), norm(string.format("%10s|%-10s|%.3s", v, v, v)))
end
print(norm(tostring(setmetatable({}, {__name = "My.Type"}))))
print(norm(tostring(setmetatable({}, {__name = ("N"):rep(100)}))))
print(norm(tostring(setmetatable({}, {__name = 42}))))
print(norm(tostring(setmetatable({}, {__name = ""}))))
print(norm(string.format("%s|%20s", setmetatable({}, {__name = "X"}), setmetatable({}, {__name = "Y"}))))
try(tostring, setmetatable({}, {__tostring = function() return 12 end}))
try(tostring, setmetatable({}, {__tostring = function() return {} end}))
try(tostring, setmetatable({}, {__tostring = function() return "custom" end}))
try(string.format, "%s", setmetatable({}, {__tostring = function() return 12.5 end}))
try(string.format, "%5s", setmetatable({}, {__tostring = function() return "a\0b" end}))
-- number metatable (debug.setmetatable) with __tostring / __name
debug.setmetatable(0, {__tostring = function(n) return "NUM" end})
print(tostring(1), tostring(2.5), string.format("%s %5s", 3, 4.5))
debug.setmetatable(0, {__name = "Numero"})
print(tostring(1), string.format("%s", 7))
debug.setmetatable(0, nil)
print(tostring(1), string.format("%s %s", 7, 8.25))
-- booleans / nil with a metatable
debug.setmetatable(nil, {__tostring = function() return "NIL!" end})
print(tostring(nil), string.format("%s", nil))
debug.setmetatable(nil, nil)
debug.setmetatable(true, {__tostring = function(b) return b and "YES" or "NO" end})
print(tostring(true), tostring(false), string.format("%s", true))
debug.setmetatable(true, nil)
print(tostring(true), tostring(nil))
-- long address digits stay lowercase hex with 0x
print(norm(string.format("%p", {})), string.format("%p", 1), string.format("%p", nil), string.format("%p", "s") == string.format("%p", "s"))
print(#tostring({}) >= 9, tostring({}):match("^table: 0x%x+$") ~= nil)
