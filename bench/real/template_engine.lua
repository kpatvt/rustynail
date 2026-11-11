-- template engine: {{var}} / {{a.b}} substitution, {% for x in list %}...{% end %}, string.format, gsub
local tpl = [[
<h1>{{title}}</h1>
<ul>
{% for item in items %}  <li class="{{item.cls}}">{{item.name}} - {{item.price}}</li>
{% end %}</ul>
<p>Total: {{total}} items by {{user.name}} ({{user.email}})</p>
]]

local function lookup(env, path)
  local v = env
  for part in path:gmatch("[^.]+") do
    if type(v) ~= "table" then return "" end
    v = v[part]
  end
  return v == nil and "" or v
end

local function escape(s)
  return (tostring(s):gsub("[<>&\"]", { ["<"] = "&lt;", [">"] = "&gt;", ["&"] = "&amp;", ['"'] = "&quot;" }))
end

local function render(t, env)
  t = t:gsub("{%% for (%w+) in (%w+) %%}(.-){%% end %%}\n?", function(var, list, body)
    local parts = {}
    for _, item in ipairs(env[list]) do
      local sub = setmetatable({ [var] = item }, { __index = env })
      parts[#parts + 1] = render(body, sub)
    end
    return table.concat(parts)
  end)
  return (t:gsub("{{%s*([%w_.]+)%s*}}", function(path) return escape(lookup(env, path)) end))
end

local items = {}
for i = 1, 60 do
  items[i] = { name = "Widget <" .. i .. ">", price = string.format("%.2f", i * 3.14159), cls = i % 2 == 0 and "even" or "odd" }
end
local env = { title = "Catalog & Stuff", items = items, total = #items, user = { name = "Ann", email = "ann@example.com" } }
local n = 0
for i = 1, 1500 do
  env.title = "Catalog " .. i
  local out = render(tpl, env)
  n = n + #out
end
print(n)
