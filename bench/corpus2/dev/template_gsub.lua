-- Template/report engine using string.format and gsub: ${var} substitution, {{#each}} blocks,
-- filters (upper, pad, money), HTML escaping; builds invoices from generated data. small: N=3
local N = tonumber(arg and arg[1]) or 10000

local seed = 8080
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- Filters -----------------------------------------------------------------------------------
local htmlEscapes = { ["&"] = "&amp;", ["<"] = "&lt;", [">"] = "&gt;", ['"'] = "&quot;", ["'"] = "&#39;" }

local function escape(s)
  return (tostring(s):gsub("[&<>\"']", htmlEscapes))
end

local function money(cents)
  local sign = cents < 0 and "-" or ""
  cents = math.abs(cents)
  local whole = tostring(math.floor(cents / 100))
  local frac = cents % 100
  whole = whole:reverse():gsub("(%d%d%d)", "%1,"):reverse()
  whole = whole:gsub("^,", "")
  return string.format("%s$%s.%02d", sign, whole, frac)
end

local filters = {
  upper = string.upper,
  lower = string.lower,
  html = escape,
  money = money,
  title = function(s) return (s:gsub("(%a)(%w*)", function(a, b) return a:upper() .. b:lower() end)) end,
  pad = function(s, n) return string.format("%-" .. (n or 10) .. "s", s) end,
  rpad = function(s, n) return string.format("%" .. (n or 10) .. "s", s) end,
  trunc = function(s, n) n = n or 8; if #s > n then return s:sub(1, n - 1) .. "~" end return s end,
}

-- Template engine ---------------------------------------------------------------------------------
local function lookup(ctx, path)
  local v = ctx
  for key in path:gmatch("[^%.]+") do
    if type(v) ~= "table" then return nil end
    v = v[key]
    if v == nil and key == "index" then v = ctx.index end
  end
  return v
end

local function applyFilters(value, chain)
  for name, arg in chain:gmatch("|%s*(%w+):?(%d*)") do
    local f = filters[name] or error("unknown filter " .. name)
    value = f(value, tonumber(arg))
  end
  return value
end

local render

local function renderVars(tpl, ctx)
  return (tpl:gsub("%${([%w_%.]+)([^}]*)}", function(path, chain)
    local v = lookup(ctx, path)
    if v == nil then return "" end
    return tostring(applyFilters(v, chain))
  end))
end

function render(tpl, ctx)
  -- {{#each list}} ... {{/each}} blocks (non-nested within the same level)
  tpl = tpl:gsub("{{#each ([%w_%.]+)}}(.-){{/each}}", function(name, body)
    local list = lookup(ctx, name) or {}
    local out = {}
    for i, item in ipairs(list) do
      local sub = setmetatable({ index = i, loop = { first = i == 1, last = i == #list } }, { __index = ctx })
      for k, v in pairs(item) do sub[k] = v end
      out[#out + 1] = render(body, sub)
    end
    return table.concat(out)
  end)
  tpl = tpl:gsub("{{#if ([%w_%.]+)}}(.-){{/if}}", function(name, body)
    if lookup(ctx, name) then return body end
    return ""
  end)
  return renderVars(tpl, ctx)
end

-- Compile a template into a closure that caches its parse (typical optimisation) ----------------------
local cache = {}
local function compile(tpl)
  local f = cache[tpl]
  if not f then
    f = function(ctx) return render(tpl, ctx) end
    cache[tpl] = f
  end
  return f
end

-- Data ---------------------------------------------------------------------------------------------------------
local firstNames = { "ada", "grace", "linus", "ken", "dennis", "barbara", "edsger", "alan" }
local lastNames = { "lovelace", "hopper", "torvalds", "thompson", "ritchie", "liskov", "dijkstra", "turing" }
local products = { "Widget <small>", "Gadget & Co", "\"Deluxe\" Gizmo", "Bolt 5/8", "O'Reilly Book", "Cable" }

local function makeInvoice(n)
  local items, total = {}, 0
  for i = 1, 1 + rnd(5) do
    local qty, price = rnd(9), rnd(20000)
    items[i] = { name = products[rnd(#products)], qty = qty, price = price, total = qty * price }
    total = total + qty * price
  end
  local tax = math.floor(total * 7 / 100)
  return {
    number = 1000 + n,
    customer = firstNames[rnd(#firstNames)] .. " " .. lastNames[rnd(#lastNames)],
    items = items,
    subtotal = total, tax = tax, grand = total + tax,
    rush = n % 4 == 0,
  }
end

local invoiceTpl = [[
Invoice #${number} for ${customer|title}
{{#each items}}  ${index}. ${name|trunc:14|pad:14} x${qty} @ ${price|money|rpad:11} = ${total|money|rpad:12}
{{/each}}Subtotal: ${subtotal|money}  Tax: ${tax|money}  Total: ${grand|money|upper}
{{#if rush}}** RUSH DELIVERY **
{{/if}}]]

local htmlTpl = [[
<tr class="inv"><td>${number}</td><td>${customer|html}</td><td>{{#each items}}<li>${name|html} (${qty})</li>{{/each}}</td><td>${grand|money}</td></tr>
]]

local totalChars, totalLines, grand = 0, 0, 0
local sampleText, sampleHtml
local renderInvoice, renderRow = compile(invoiceTpl), compile(htmlTpl)
for n = 1, N do
  local inv = makeInvoice(n)
  local text = renderInvoice(inv)
  local row = renderRow(inv)
  totalChars = totalChars + #text + #row
  grand = grand + inv.grand
  for _ in text:gmatch("\n") do totalLines = totalLines + 1 end
  if n == 1 then sampleText, sampleHtml = text, row end
  if n == 4 and not sampleHtml:find("RUSH") then sampleHtml = sampleHtml .. row end
end

io.write(sampleText)
print(sampleHtml:sub(1, 160))
print(string.format("invoices=%d lines=%d chars=%d grand=%s", N, totalLines, totalChars, money(grand)))

-- Simple key=value report using format and gsub -----------------------------------------------------------------
local stats = {}
local reportTpl = "${name|pad:12}|${count|rpad:6}|${amount|money|rpad:14}|${note|html}"
for i = 1, 6 do
  stats[i] = { name = lastNames[i], count = rnd(500), amount = rnd(10000000) - 5000000, note = "<b>" .. i .. "</b> & more" }
end
for _, s in ipairs(stats) do print(compile(reportTpl)(s)) end

print(money(0), money(5), money(123456789), money(-9876543210))
print(escape("a<b & c>d \"q\" 'r'"))
print((renderVars("${missing}${x|upper}!", { x = "ok" })))
local good, msg = pcall(renderVars, "${x|nofilter}", { x = 1 })
print(good, (tostring(msg):gsub("^.-:%d+: ", "")))
