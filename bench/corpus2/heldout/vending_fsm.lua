-- vending_fsm: imitates table-driven finite state machines: vending machine, traffic-light
-- controller and an HTTP-like request parser, transitions in nested tables. small: N=1
local N = tonumber(arg and arg[1]) or 620

local seed = 4242
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- generic engine: def.transitions[state][event] = { to = state, do = actionname }
local function make_machine(def, ctx)
  local m = { state = def.initial, ctx = ctx, visits = {}, ignored = 0, steps = 0 }
  m.visits[m.state] = 1
  function m.fire(event, arg)
    local row = def.transitions[m.state]
    local t = row and row[event]
    if not t then
      m.ignored = m.ignored + 1
      return false
    end
    m.steps = m.steps + 1
    if t.act then def.actions[t.act](ctx, arg, m) end
    m.state = t.to
    m.visits[t.to] = (m.visits[t.to] or 0) + 1
    return true
  end
  return m
end

local function sorted_keys(t)
  local ks = {}
  for k in pairs(t) do ks[#ks + 1] = k end
  table.sort(ks)
  return ks
end

local function report(name, m)
  local parts = {}
  for _, k in ipairs(sorted_keys(m.visits)) do parts[#parts + 1] = k .. "=" .. m.visits[k] end
  print(string.format("%s: final=%s steps=%d ignored=%d visits[%s]", name, m.state, m.steps, m.ignored,
    table.concat(parts, " ")))
end

-- ------------------------------------------------------------ 1. vending machine
local PRICES = { A = 60, B = 75, C = 100 }
local vend = {
  initial = "idle",
  actions = {
    add_coin = function(c, v) c.credit = c.credit + v end,
    start_vend = function(c, item)
      c.item = item
      c.stock[item] = c.stock[item] - 1
    end,
    finish_vend = function(c)
      c.credit = c.credit - PRICES[c.item]
      c.sales[c.item] = (c.sales[c.item] or 0) + 1
      c.revenue = c.revenue + PRICES[c.item]
      c.item = nil
    end,
    give_change = function(c)
      c.refunded = c.refunded + c.credit
      c.credit = 0
    end,
    restock = function(c)
      for k in pairs(c.stock) do c.stock[k] = 5 end
    end,
    break_down = function(c) c.faults = c.faults + 1 end,
  },
  transitions = {
    idle = {
      coin = { to = "has_money", act = "add_coin" },
      service = { to = "service", act = "break_down" },
    },
    has_money = {
      coin = { to = "has_money", act = "add_coin" },
      select = { to = "dispensing", act = "start_vend" },
      refund = { to = "idle", act = "give_change" },
    },
    dispensing = {
      done = { to = "change", act = "finish_vend" },
      jam = { to = "service", act = "break_down" },
    },
    change = {
      tick = { to = "idle", act = "give_change" },
      coin = { to = "has_money", act = "add_coin" },
    },
    service = {
      restock = { to = "idle", act = "restock" },
    },
  },
}

local function run_vending(events)
  local ctx = { credit = 0, stock = { A = 5, B = 5, C = 5 }, sales = {}, revenue = 0, refunded = 0, faults = 0 }
  local m = make_machine(vend, ctx)
  local coins = { 5, 10, 25, 50, 100 }
  for _ = 1, events do
    local r = rnd(100)
    if r <= 40 then
      m.fire("coin", coins[rnd(5)])
    elseif r <= 60 then
      local item = ({ "A", "B", "C" })[rnd(3)]
      -- the select event only makes sense with enough credit and stock
      if ctx.credit >= PRICES[item] and ctx.stock[item] > 0 then m.fire("select", item) else m.fire("refund") end
    elseif r <= 80 then
      if m.state == "dispensing" and rnd(20) == 1 then m.fire("jam") else m.fire("done") end
    elseif r <= 92 then
      m.fire("tick")
    elseif r <= 96 then
      m.fire("service")
    else
      m.fire("restock")
    end
  end
  report("vending", m)
  print(string.format("  revenue=%d refunded=%d credit=%d faults=%d sales A/B/C=%d/%d/%d", ctx.revenue,
    ctx.refunded, ctx.credit, ctx.faults, ctx.sales.A or 0, ctx.sales.B or 0, ctx.sales.C or 0))
end

-- ------------------------------------------------------------ 2. traffic light controller
local DURATION = { red = 6, green = 5, yellow = 2, flashing = 4 }
local light = {
  initial = "red",
  actions = {
    reset_timer = function(c, _, m) c.timer = 0 end,
    count_cycle = function(c) c.cycles = c.cycles + 1; c.timer = 0 end,
    emergency = function(c) c.emergencies = c.emergencies + 1; c.timer = 0 end,
    flash = function(c) c.faults = c.faults + 1; c.timer = 0 end,
  },
  transitions = {
    red = { expire = { to = "green", act = "reset_timer" }, emergency = { to = "red", act = "emergency" },
            fault = { to = "flashing", act = "flash" } },
    green = { expire = { to = "yellow", act = "reset_timer" }, emergency = { to = "red", act = "emergency" },
              fault = { to = "flashing", act = "flash" } },
    yellow = { expire = { to = "red", act = "count_cycle" }, emergency = { to = "red", act = "emergency" },
               fault = { to = "flashing", act = "flash" } },
    flashing = { expire = { to = "red", act = "reset_timer" } },
  },
}

local function run_traffic(ticks)
  local ctx = { timer = 0, cycles = 0, emergencies = 0, faults = 0 }
  local m = make_machine(light, ctx)
  local time_in = { red = 0, green = 0, yellow = 0, flashing = 0 }
  for _ = 1, ticks do
    ctx.timer = ctx.timer + 1
    time_in[m.state] = time_in[m.state] + 1
    local r = rnd(1000)
    if r == 1 then
      m.fire("fault")
    elseif r <= 6 then
      m.fire("emergency")
    elseif ctx.timer >= DURATION[m.state] then
      m.fire("expire")
    end
  end
  report("traffic", m)
  print(string.format("  cycles=%d emergencies=%d faults=%d time red/green/yellow/flash=%d/%d/%d/%d", ctx.cycles,
    ctx.emergencies, ctx.faults, time_in.red, time_in.green, time_in.yellow, time_in.flashing))
end

-- ------------------------------------------------------------ 3. protocol parser
local function classify(b)
  if b == 32 then return "space"
  elseif b == 13 then return "cr"
  elseif b == 10 then return "lf"
  elseif b == 58 then return "colon"
  elseif (b >= 65 and b <= 90) or (b >= 97 and b <= 122) or (b >= 48 and b <= 57) or b == 45 or b == 46 or b == 47 or b == 95 then
    return "word"
  end
  return "bad"
end

local parser = {
  initial = "method",
  actions = {
    add = function(c, ch) c.buf[#c.buf + 1] = ch end,
    set_method = function(c) c.req.method = table.concat(c.buf); c.buf = {} end,
    set_path = function(c) c.req.path = table.concat(c.buf); c.buf = {} end,
    set_version = function(c) c.req.version = table.concat(c.buf); c.buf = {} end,
    set_hname = function(c) c.hname = table.concat(c.buf):lower(); c.buf = {} end,
    set_hvalue = function(c)
      c.req.headers[c.hname] = table.concat(c.buf); c.buf = {}
      c.req.nheaders = c.req.nheaders + 1
    end,
    done = function(c) c.done = true end,
    fail = function(c) c.failed = true end,
  },
  transitions = {
    method = { word = { to = "method", act = "add" }, space = { to = "path", act = "set_method" }, bad = { to = "error", act = "fail" } },
    path = { word = { to = "path", act = "add" }, space = { to = "version", act = "set_path" }, bad = { to = "error", act = "fail" } },
    version = { word = { to = "version", act = "add" }, cr = { to = "version_lf", act = "set_version" } },
    version_lf = { lf = { to = "header_start" } },
    header_start = { word = { to = "hname", act = "add" }, cr = { to = "end_lf" } },
    hname = { word = { to = "hname", act = "add" }, colon = { to = "hspace", act = "set_hname" } },
    hspace = { space = { to = "hspace" }, word = { to = "hvalue", act = "add" } },
    hvalue = { word = { to = "hvalue", act = "add" }, space = { to = "hvalue", act = "add" }, cr = { to = "hvalue_lf", act = "set_hvalue" } },
    hvalue_lf = { lf = { to = "header_start" } },
    end_lf = { lf = { to = "complete", act = "done" } },
  },
}

local METHODS = { "GET", "POST", "PUT", "DELETE", "HEAD" }
local SEGS = { "index", "users", "api", "v1", "items", "42", "search", "img.png", "a_b" }
local HNAMES = { "Host", "User-Agent", "Accept", "Content-Length", "X-Trace", "Cache-Control" }

local function gen_message(i)
  local out = { METHODS[rnd(#METHODS)], " /" }
  for _ = 1, rnd(3) do out[#out + 1] = SEGS[rnd(#SEGS)] .. "/" end
  out[#out + 1] = " HTTP/1." .. (rnd(2) - 1) .. "\r\n"
  for _ = 1, rnd(5) do
    out[#out + 1] = HNAMES[rnd(#HNAMES)] .. ": val " .. rnd(1000) .. " " .. SEGS[rnd(#SEGS)] .. "\r\n"
  end
  out[#out + 1] = "\r\n"
  local s = table.concat(out)
  local r = rnd(10)
  if r == 1 then s = s:sub(1, math.floor(#s / 2)) end            -- truncated
  if r == 2 then s = s:gsub("HTTP", "HT?P", 1) end               -- bad character
  if r == 3 then s = s:gsub(": ", " ", 1) end                    -- missing colon
  return s
end

local function parse(msg)
  local ctx = { buf = {}, req = { headers = {}, nheaders = 0 } }
  local m = make_machine(parser, ctx)
  for i = 1, #msg do
    local ch = msg:sub(i, i)
    if not m.fire(classify(msg:byte(i)), ch) then
      return nil, "unexpected " .. classify(msg:byte(i)) .. " in " .. m.state .. " at " .. i
    end
    if ctx.failed then return nil, "bad character at " .. i end
  end
  if not ctx.done then return nil, "incomplete in " .. m.state end
  return ctx.req
end

local function run_parser(count)
  local ok, bad, headers, bytes = 0, 0, 0, 0
  local by_method, errors, sample = {}, {}, nil
  for i = 1, count do
    local msg = gen_message(i)
    bytes = bytes + #msg
    local req, err = parse(msg)
    if req then
      ok = ok + 1
      headers = headers + req.nheaders
      by_method[req.method] = (by_method[req.method] or 0) + 1
      if i == 1 or not sample then sample = req end
    else
      bad = bad + 1
      local kind = err:match("^(%a+ %a*)")
      errors[kind] = (errors[kind] or 0) + 1
    end
  end
  print(string.format("parser: messages=%d ok=%d bad=%d headers=%d bytes=%d", count, ok, bad, headers, bytes))
  local parts = {}
  for _, k in ipairs(sorted_keys(by_method)) do parts[#parts + 1] = k .. "=" .. by_method[k] end
  print("  methods " .. table.concat(parts, " "))
  parts = {}
  for _, k in ipairs(sorted_keys(errors)) do parts[#parts + 1] = k .. "=" .. errors[k] end
  print("  errors " .. table.concat(parts, "; "))
  if sample then
    print("  sample " .. sample.method .. " " .. sample.path .. " " .. sample.version)
    for _, k in ipairs(sorted_keys(sample.headers)) do print("    " .. k .. " = " .. sample.headers[k]) end
  end
end

run_vending(N * 2500)
run_traffic(N * 4000)
run_parser(N * 25)
