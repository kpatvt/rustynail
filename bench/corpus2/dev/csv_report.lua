-- CSV data processing: generate CSV with quoted fields, parse it, group-by/aggregate,
-- sort and print a formatted report. small: N=20
local N = tonumber(arg and arg[1]) or 70000

local seed = 99
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

local regions = { "North", "South", "East", "West", "Central" }
local products = { "Widget", "Gadget, large", "Doohickey", "\"Premium\" Gizmo", "Thingamajig" }
local reps = { "Alice Smith", "Bob O'Neil", "Carol, Jr.", "Dave Wu", "Eve Adams", "Frank Li" }

-- Generation ------------------------------------------------------------------
local function quote(s)
  if s:find('[",\n]') then
    return '"' .. s:gsub('"', '""') .. '"'
  end
  return s
end

local function generate(rows)
  local out = { "id,region,product,rep,qty,price,note" }
  for i = 1, rows do
    local price = rnd(9000) + 99
    local note = ""
    if rnd(5) == 1 then note = "rush order #" .. i end
    if rnd(9) == 1 then note = "said \"call back\", twice" end
    out[#out + 1] = table.concat({
      tostring(i), regions[rnd(#regions)], quote(products[rnd(#products)]),
      quote(reps[rnd(#reps)]), tostring(rnd(20)),
      string.format("%d.%02d", math.floor(price / 100), price % 100), quote(note),
    }, ",")
  end
  return table.concat(out, "\n") .. "\n"
end

-- Parsing (handles quoted fields with embedded commas and doubled quotes) ----
local function parseLine(line)
  local fields, pos, len = {}, 1, #line
  while pos <= len + 1 do
    local c = line:sub(pos, pos)
    if c == '"' then
      local value = {}
      pos = pos + 1
      while true do
        local q = line:find('"', pos, true)
        if not q then error("unterminated quote") end
        value[#value + 1] = line:sub(pos, q - 1)
        if line:sub(q + 1, q + 1) == '"' then
          value[#value + 1] = '"'
          pos = q + 2
        else
          pos = q + 1
          break
        end
      end
      fields[#fields + 1] = table.concat(value)
      pos = pos + 1
    else
      local comma = line:find(",", pos, true)
      if comma then
        fields[#fields + 1] = line:sub(pos, comma - 1)
        pos = comma + 1
      else
        fields[#fields + 1] = line:sub(pos)
        pos = len + 2
      end
    end
  end
  return fields
end

local function parseCSV(text)
  local header, records = nil, {}
  for line in text:gmatch("([^\n]*)\n") do
    local f = parseLine(line)
    if not header then
      header = f
    else
      local rec = {}
      for i, name in ipairs(header) do rec[name] = f[i] end
      rec.qty = tonumber(rec.qty)
      rec.price = tonumber(rec.price)
      records[#records + 1] = rec
    end
  end
  return records
end

-- Aggregation -------------------------------------------------------------------
local function groupBy(records, keyfn)
  local groups, order = {}, {}
  for _, r in ipairs(records) do
    local k = keyfn(r)
    local g = groups[k]
    if not g then
      g = { key = k, count = 0, sum = 0, min = math.huge, max = -math.huge, units = 0 }
      groups[k] = g
      order[#order + 1] = g
    end
    local total = r.qty * r.price
    g.count = g.count + 1
    g.sum = g.sum + total
    g.units = g.units + r.qty
    if total < g.min then g.min = total end
    if total > g.max then g.max = total end
  end
  return order
end

local function commas(n)
  local s = string.format("%.2f", n)
  local int, frac = s:match("^(%d+)(%.%d+)$")
  int = int:reverse():gsub("(%d%d%d)", "%1,"):reverse():gsub("^,", "")
  return int .. frac
end

local function report(title, groups, sortKey)
  table.sort(groups, function(a, b)
    if a[sortKey] ~= b[sortKey] then return a[sortKey] > b[sortKey] end
    return a.key < b.key
  end)
  print(string.format("== %s (by %s) ==", title, sortKey))
  print(string.format("%-22s %6s %14s %10s %9s %9s", "key", "count", "sum", "mean", "min", "max"))
  for _, g in ipairs(groups) do
    print(string.format("%-22s %6d %14s %10.2f %9.2f %9.2f",
      g.key, g.count, commas(g.sum), g.sum / g.count, g.min, g.max))
  end
end

-- Main ---------------------------------------------------------------------------
local text = generate(N)
local records = parseCSV(text)
print(string.format("csv bytes=%d rows=%d", #text, #records))

local quoted = 0
for _, r in ipairs(records) do
  if r.note:find('"', 1, true) or r.product:find(",", 1, true) then quoted = quoted + 1 end
end
print("rows with tricky fields: " .. quoted)

report("Region", groupBy(records, function(r) return r.region end), "sum")
report("Product", groupBy(records, function(r) return r.product end), "count")
report("Rep", groupBy(records, function(r) return r.rep end), "units")
report("Region/Rep top", (function()
  local gs = groupBy(records, function(r) return r.region .. "/" .. r.rep end)
  table.sort(gs, function(a, b)
    if a.sum ~= b.sum then return a.sum > b.sum end
    return a.key < b.key
  end)
  local top = {}
  for i = 1, 5 do top[i] = gs[i] end
  return top
end)(), "sum")

-- Round trip checksum: re-serialize and compare with the original text.
local lines = {}
for _, r in ipairs(records) do
  lines[#lines + 1] = table.concat({ r.id, r.region, quote(r.product), quote(r.rep),
    r.qty, string.format("%.2f", r.price), quote(r.note) }, ",")
end
local rebuilt = "id,region,product,rep,qty,price,note\n" .. table.concat(lines, "\n") .. "\n"
print("round trip identical: " .. tostring(rebuilt == text))
