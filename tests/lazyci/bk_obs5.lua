-- (bk_obs5 = bk_obs3 with a collector step (the heap verifier checks the CallInfo chain after it) inside the inlined callee, i.e. between the exit and the next leave: detector of the `top` / `kcache` half of `lazy_mat_no_leave`, run with RUSTYNAIL_GC_VERIFY=1)
-- BUNDLE-K L1 (lazy CallInfo): an exit inside an INLINED callee of a native recursion. The leaf helper is inlined into the
-- recursion's trace; once it is hot its guard (`mode`) starts to fail at some leaves, at depth, while the lazy frames of the chain are
-- incomplete: the inline frame-rebuilding stub (`mat_stub`) must complete the chain first. The observer (getinfo of every level,
-- getlocal, traceback) sees the whole chain. Every output must equal the reference interpreter's (detector of `lazy_mat_no_leave`).
local function norm(s) return (tostring(s):gsub("0x%x+", "PTR")) end
local mode, seen = 0, 0
local function leaf(n)
  if mode == 0 then return n end
  seen = seen + 1
  collectgarbage("step", 0)
  if seen % 3 == 0 then
    local t = {}
    for lv = 1, 40 do
      local i = debug.getinfo(lv, "Sln")
      if not i then break end
      t[#t + 1] = i.currentline .. ":" .. tostring(i.name) .. (debug.getinfo(lv, "t").istailcall and "T" or "")
    end
    local _, v = debug.getlocal(2, 1)
    io.write(table.concat(t, ","), " n=", tostring(v), "\n")
    if seen % 9 == 0 then print((norm(debug.traceback("tb", 1)):gsub("\n", " | "))) end
  end
  return n + 0
end
local cnt = 0
local function f(n)
  if n < 2 then
    cnt = cnt + 1
    local r = leaf(n)
    return r
  end
  local a = f(n - 1)
  local b = f(n - 2)
  cnt = cnt + (a - b) % 2
  return a + b
end
local function warm() for _ = 1, 40 do f(14) end end
-- The CallInfo nodes a lazy chain pushes keep whatever an earlier call left in the fields the native push does not store (`callstatus`
-- with CIST_TAIL, `kcache`, `top`, `trap`): `scramble` runs, in a fresh prototype (cold, so its frames are eager), a chain of calls whose
-- frames were entered by tail calls and whose constants differ from `f`'s, at the depths `f` uses. A chain that is not completed would show
-- `istailcall` in the observers or use the wrong constants after a return (`f`'s `% 2`: `cnt` is printed).
local scr_src = [[
local fb
local function fa(n)
  if n == 0 then return 0 end
  local r = fb(n - 1)
  return r + 123456
end
function fb(n) return fa(n) end
return fa(60)
]]
local function scramble() return load(scr_src)() end
local fb
local function fa(n)
  if n == 0 then return 0 end
  local r = fb(n - 1)
  return r + 1
end
function fb(n) return fa(n) end
local function fill() return fa(60) end
warm()
fill()
scramble()
print(f(12))
mode = 1
fill() scramble()
print(f(10))
fill() scramble()
print(f(11))
fill() scramble()
print(f(9))
mode = 0
print(f(13))
mode = 1
for i = 1, 6 do fill() scramble() f(8 + i % 3) end
print(seen, cnt)
