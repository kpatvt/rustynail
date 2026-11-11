-- O0: distinct Lua callees per call site (line-level), via the call hook. usage: rnlua o0hook.lua <prog>
local prog = arg[1]
local LIMIT = tonumber(os.getenv("O0_SECS") or "12")
local sites = {}
local ncalls = 0
local getinfo = debug.getinfo
local clock = os.clock
local t0 = clock()
local function report()
  debug.sethook()
  local out = {}
  for site, s in pairs(sites) do
    local n = 0
    for _ in pairs(s.callees) do n = n + 1 end
    out[#out + 1] = string.format("SITE %d %d %s", n, s.n, site)
  end
  table.sort(out)
  io.stderr:write("O0CALLS ", ncalls, "\n")
  for _, l in ipairs(out) do io.stderr:write(l, "\n") end
end
local function hook(ev)
  ncalls = ncalls + 1
  if ncalls % 65536 == 0 and clock() - t0 > LIMIT then
    report()
    os.exit(0, true)
  end
  local callee = getinfo(2, "S")
  if callee.what ~= "Lua" then return end
  if ev == "tail call" then return end
  local caller = getinfo(3, "Sl")
  if not caller or caller.what ~= "Lua" then return end
  local site = caller.short_src .. ":" .. caller.currentline
  local s = sites[site]
  if not s then s = { n = 0, callees = {} }; sites[site] = s end
  s.n = s.n + 1
  s.callees[callee.short_src .. ":" .. callee.linedefined] = true
end
local f = assert(loadfile(prog))
local a = { [0] = prog }
arg = a
debug.sethook(hook, "c")
f()
report()
