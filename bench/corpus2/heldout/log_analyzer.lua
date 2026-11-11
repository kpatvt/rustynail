-- log_analyzer: imitates a web access-log analysis script (string.match/gmatch parsing, per
-- status/path/hour aggregation, percentiles, top-N report). small: N=1
local N = tonumber(arg and arg[1]) or 170

local seed = 99991
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end
local function pick(t) return t[rnd(#t)] end

-- ------------------------------------------------------------ line generator
local METHODS = { "GET", "GET", "GET", "GET", "POST", "PUT", "DELETE", "HEAD" }
local PATHS = { "/", "/index.html", "/api/users", "/api/orders", "/api/orders/42", "/static/app.js",
  "/static/style.css", "/login", "/logout", "/search", "/img/logo.png", "/admin", "/healthz" }
local STATUS = { 200, 200, 200, 200, 200, 200, 301, 304, 304, 404, 404, 403, 500, 502 }
local AGENTS = { "Mozilla/5.0 (X11; Linux x86_64) Firefox/115.0", "curl/7.88.1",
  "Mozilla/5.0 (Windows NT 10.0) Chrome/118.0 Safari/537.36", "Googlebot/2.1 (+http://www.google.com/bot.html)",
  "python-requests/2.31" }
local MONTHS = { "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec" }
local QUERIES = { "", "", "", "?page=%d", "?q=lua&page=%d&sort=asc", "?id=%d&debug=1" }

local function gen_line()
  local r = rnd(100)
  if r <= 2 then return "garbage line without structure " .. rnd(1000) end
  local ip = string.format("%d.%d.%d.%d", 10 + rnd(3), rnd(6), rnd(6), rnd(50))
  local q = pick(QUERIES)
  if q:find("%%d") then q = q:gsub("%%d", tostring(rnd(30))) end
  local status = pick(STATUS)
  local bytes = status == 304 and 0 or rnd(5000) * rnd(4) + rnd(100)
  return string.format('%s - - [%02d/%s/2023:%02d:%02d:%02d +0000] "%s %s%s HTTP/1.1" %d %d "-" "%s"',
    ip, rnd(28), MONTHS[rnd(12)], rnd(24) - 1, rnd(60) - 1, rnd(60) - 1, pick(METHODS), pick(PATHS), q, status, bytes, pick(AGENTS))
end

-- ------------------------------------------------------------ parsing
local PATTERN = '^(%d+%.%d+%.%d+%.%d+) %S+ %S+ %[(%d+)/(%a+)/(%d+):(%d+):(%d+):(%d+) [^%]]+%] "(%u+) (%S+) HTTP/[%d%.]+" (%d+) (%d+) "([^"]*)" "([^"]*)"$'

local function parse_line(line)
  local ip, day, mon, year, hh, mm, ss, method, url, status, bytes, ref, agent = line:match(PATTERN)
  if not ip then return nil end
  local path, query = url:match("^([^?]*)%??(.*)$")
  local params = {}
  for k, v in query:gmatch("([%w_]+)=([%w_]*)") do params[k] = v end
  return {
    ip = ip, day = tonumber(day), mon = mon, hour = tonumber(hh), minute = tonumber(mm),
    method = method, path = path, params = params, status = tonumber(status),
    bytes = tonumber(bytes), agent = agent,
  }
end

local function browser_of(agent)
  if agent:find("Googlebot", 1, true) then return "bot" end
  if agent:find("curl", 1, true) or agent:find("python", 1, true) then return "script" end
  if agent:find("Firefox") then return "firefox" end
  if agent:find("Chrome") then return "chrome" end
  return "other"
end

-- ------------------------------------------------------------ aggregation helpers
local function bump(t, k, n) t[k] = (t[k] or 0) + (n or 1) end

local function sorted_by_count(t, limit)
  local keys = {}
  for k in pairs(t) do keys[#keys + 1] = k end
  table.sort(keys, function(a, b)
    if t[a] ~= t[b] then return t[a] > t[b] end
    return tostring(a) < tostring(b)
  end)
  local out = {}
  for i = 1, math.min(limit or #keys, #keys) do out[i] = keys[i] end
  return out
end

local function percentile(sorted, p)
  local idx = math.floor((#sorted - 1) * p / 100) + 1
  return sorted[idx]
end

-- ------------------------------------------------------------ main pass
local total, bad = N * 1000, 0
local by_status, by_path, by_hour, by_ip, by_method, by_browser, by_param = {}, {}, {}, {}, {}, {}, {}
local bytes_by_path, errors_by_path = {}, {}
local all_bytes, sizes_200 = {}, {}
local total_bytes, checksum = 0, 0
local minute_load = {}

for i = 1, total do
  local line = gen_line()
  local rec = parse_line(line)
  if not rec then
    bad = bad + 1
  else
    bump(by_status, rec.status)
    bump(by_path, rec.path)
    bump(by_hour, rec.hour)
    bump(by_ip, rec.ip)
    bump(by_method, rec.method)
    bump(by_browser, browser_of(rec.agent))
    bump(bytes_by_path, rec.path, rec.bytes)
    if rec.status >= 500 then bump(errors_by_path, rec.path) end
    for k in pairs(rec.params) do bump(by_param, k) end
    all_bytes[#all_bytes + 1] = rec.bytes
    if rec.status == 200 then sizes_200[#sizes_200 + 1] = rec.bytes end
    total_bytes = total_bytes + rec.bytes
    bump(minute_load, rec.hour * 60 + rec.minute)
    checksum = (checksum * 31 + rec.status + rec.bytes + #rec.path) % 1000000007
  end
end

print(string.format("lines=%d parsed=%d malformed=%d total_bytes=%d checksum=%d", total, total - bad, bad, total_bytes, checksum))

print("-- status codes")
local codes = {}
for k in pairs(by_status) do codes[#codes + 1] = k end
table.sort(codes)
for _, c in ipairs(codes) do
  print(string.format("  %d %6d %5.1f%%", c, by_status[c], by_status[c] * 100 / (total - bad)))
end

print("-- top paths")
for _, p in ipairs(sorted_by_count(by_path, 6)) do
  print(string.format("  %-18s hits=%5d bytes=%9d 5xx=%d", p, by_path[p], bytes_by_path[p], errors_by_path[p] or 0))
end

print("-- top clients")
for _, ip in ipairs(sorted_by_count(by_ip, 5)) do print(string.format("  %-16s %d", ip, by_ip[ip])) end

print("-- hourly histogram")
local hours = {}
for h in pairs(by_hour) do hours[#hours + 1] = h end
table.sort(hours)
local peak_hour, peak = -1, -1
for _, h in ipairs(hours) do
  if h % 6 == 0 then print(string.format("  %02d:00 %s", h, string.rep("#", math.floor(by_hour[h] / (total / 400))))) end
  if by_hour[h] > peak then peak_hour, peak = h, by_hour[h] end
end
print(string.format("  peak hour %02d with %d requests", peak_hour, peak))

local busiest_min, busy = 0, -1
for m = 0, 1439 do
  local v = minute_load[m] or 0
  if v > busy then busiest_min, busy = m, v end
end
print(string.format("busiest minute %02d:%02d with %d requests", math.floor(busiest_min / 60), busiest_min % 60, busy))

print("-- methods / browsers / query params")
for _, name in ipairs({ "method", "browser", "param" }) do
  local t = ({ method = by_method, browser = by_browser, param = by_param })[name]
  local parts = {}
  for _, k in ipairs(sorted_by_count(t)) do parts[#parts + 1] = k .. "=" .. t[k] end
  print("  " .. name .. ": " .. table.concat(parts, " "))
end

print("-- response size percentiles")
table.sort(all_bytes)
table.sort(sizes_200)
for _, p in ipairs({ 50, 90, 95, 99 }) do
  print(string.format("  p%d all=%d ok200=%d", p, percentile(all_bytes, p), percentile(sizes_200, p)))
end
local sum = 0
for _, b in ipairs(all_bytes) do sum = sum + b end
print(string.format("  mean=%.2f max=%d min=%d", sum / #all_bytes, all_bytes[#all_bytes], all_bytes[1]))
