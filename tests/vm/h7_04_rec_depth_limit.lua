-- H7: recursion until "stack overflow": the depth reached and the message must equal the interpreter's
local d = 0
local function f(n) d = d + 1 return 1 + f(n + 1) end
local ok, e = pcall(f, 1)
print(ok, d - d % 1000, (tostring(e):gsub("^[^:]*:%d+: ", "")):sub(1, 60))
local d2 = 0
local function g(a, b, c) d2 = d2 + 1 local x, y, z = a, b, c return g(x, y, z) + 1 end
local ok2, e2 = pcall(g, 1, 2, 3)
print(ok2, d2, (tostring(e2):gsub("^[^:]*:%d+: ", "")):sub(1, 60))
-- near the limit, repeatedly
local best = 0
local function h(n, lim) if n >= lim then return n end return h(n + 1, lim) + 0 end
for _, lim in ipairs { 1000, 10000, 100000, 150000 } do
  local ok3, r = pcall(h, 1, lim)
  print(lim, ok3, ok3 and r or "overflow")
end
-- C stack (pcall nesting) limit
local nest = 0
local function p() nest = nest + 1 local ok = pcall(p) if not ok then error("x") end end
local ok4 = pcall(p)
print(ok4, nest > 100)
