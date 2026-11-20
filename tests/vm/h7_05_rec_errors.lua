-- H7: errors at depth, pcall at every level, tracebacks through recursion
local function strip(s) return (s:gsub("\n\t[^\n]-:%d+:", "\n\t"):gsub("^[^\n]-:%d+: ", "")) end
local function boom(n) if n == 0 then error("bottom") end return boom(n - 1) + 1 end
for _, n in ipairs { 1, 7, 100, 400 } do print(n, pcall(boom, n)) end
local function tb(n) if n == 0 then return (debug.traceback("tb", 1)) end return (tb(n - 1)) end
local t = tb(6)
local lines = 0 for _ in t:gmatch("\n") do lines = lines + 1 end
print(lines >= 8, (strip(t):match("^[^\n]*\n[^\n]*\n[^\n]*\n[^\n]*")))
local function pc(n) if n == 0 then error({ code = 42 }) end local ok, e = pcall(pc, n - 1) if n % 3 == 0 then error(e) end return ok, e end
for i = 1, 30 do local ok, e = pcall(pc, i) io.write(tostring(ok), ":", type(e) == "table" and e.code or tostring(e), " ") end print()
local function ex(n) if n == 0 then return xpcall(function() error("deep") end, function(m) return "H:" .. m:gsub("^[^:]*:%d+: ", "") end) end return ex(n - 1) end
print(ex(50))
local function fin(n) if n == 0 then error("E", 0) end local ok, e = pcall(fin, n - 1) return e .. n end
print(fin(15))
for i = 1, 300 do assert(select(2, pcall(boom, i % 40)) :find("bottom")) end
print("done")
