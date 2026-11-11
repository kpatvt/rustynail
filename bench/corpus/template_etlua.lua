-- etlua-style template compiler: templates are compiled to Lua source and loaded with an environment.
local concat, insert = table.concat, table.insert
local load = load

local html_escape_entities = {
  ['&'] = '&amp;', ['<'] = '&lt;', ['>'] = '&gt;', ['"'] = '&quot;', ["'"] = '&#039;',
}
local function html_escape(str)
  return (str:gsub([=[["><'&]]=], html_escape_entities))
end

local function get_line(str, line_num)
  for line in str:gmatch("([^\n]*)\n?") do
    if line_num == 1 then return line end
    line_num = line_num - 1
  end
end

local Parser = {}
Parser.__index = Parser

function Parser.new()
  return setmetatable({
    open_tag = "<%", close_tag = "%>", modifiers = "^[=-]",
    html_escape = true,
  }, Parser)
end

function Parser:next_tag()
  local start, stop = self.str:find(self.open_tag, self.pos, true)
  if not start then
    self:push_raw(self.str:sub(self.pos, -1))
    return false
  end
  if start ~= self.pos then
    self:push_raw(self.str:sub(self.pos, start - 1))
  end
  self.pos = stop + 1
  local modifier
  if self.str:match(self.modifiers, self.pos) then
    modifier = self.str:sub(self.pos, self.pos)
    self.pos = self.pos + 1
  end
  local close_start, close_stop = self.str:find(self.close_tag, self.pos, true)
  if not close_start then
    return nil, self:error_for_pos(start, "failed to find closing tag")
  end
  -- skip over strings inside the tag
  local chunk = self.str:sub(self.pos, close_start - 1)
  local trim_newline
  if chunk:sub(-1) == "-" then
    chunk = chunk:sub(1, -2)
    trim_newline = true
  end
  self:push_code(modifier or "code", self.pos, chunk)
  self.pos = close_stop + 1
  if trim_newline then
    local match = self.str:match("^\n", self.pos)
    if match then self.pos = self.pos + #match end
  end
  return true
end

function Parser:error_for_pos(source_pos, err_msg)
  local source_line_no = 1
  for _ in self.str:sub(1, source_pos):gmatch("\n") do
    source_line_no = source_line_no + 1
  end
  local source_line = get_line(self.str, source_line_no)
  return tostring(err_msg) .. " [" .. tostring(source_line_no) .. "]: " .. tostring(source_line)
end

function Parser:push_raw(text)
  insert(self.chunks, text)
end

function Parser:push_code(kind, start, text)
  insert(self.chunks, { kind, start, text })
end

function Parser:parse(str)
  self.str = str
  self.pos = 1
  self.chunks = {}
  while true do
    local found, err = self:next_tag()
    if err then return nil, err end
    if not found then break end
  end
  return self.chunks
end

function Parser:chunks_to_lua()
  local r = self.chunks
  local buffer = { "local _b, _b_i, _tostring, _escape = ...\n" }
  local function push(s) buffer[#buffer + 1] = s end
  for i = 1, #r do
    local chunk = r[i]
    local t = type(chunk)
    if t == "table" then
      t = chunk[1]
    end
    if t == "string" then
      push("_b_i = _b_i + 1\n")
      push("_b[_b_i] = " .. string.format("%q", chunk) .. "\n")
    elseif t == "code" then
      push(chunk[3] .. "\n")
    elseif t == "=" or t == "-" then
      push("_b_i = _b_i + 1\n")
      if t == "=" and self.html_escape then
        push("_b[_b_i] = _escape(_tostring(" .. chunk[3] .. "))\n")
      else
        push("_b[_b_i] = _tostring(" .. chunk[3] .. ")\n")
      end
    else
      error("unknown type " .. tostring(t))
    end
  end
  push("return _b")
  return concat(buffer)
end

function Parser:compile(str)
  local chunks, err = self:parse(str)
  if not chunks then return nil, err end
  local code = self:chunks_to_lua()
  return function(env)
    local scope = setmetatable({}, { __index = function(_, k)
      local v = env[k]
      if v == nil then v = _G[k] end
      return v
    end })
    local fn, lerr = load(code, "etlua", "t", scope)
    if not fn then error(lerr) end
    local buf = fn({}, 0, tostring, html_escape)
    return concat(buf)
  end, code
end

-- A cached compiled version: compile the code once and reuse the loaded function with a fresh env.
function Parser:compile_cached(str)
  local chunks, err = self:parse(str)
  if not chunks then return nil, err end
  local code = self:chunks_to_lua()
  local current_env
  local scope = setmetatable({}, { __index = function(_, k)
    local v = current_env[k]
    if v == nil then v = _G[k] end
    return v
  end })
  local fn = assert(load(code, "etlua_cached", "t", scope))
  return function(env)
    current_env = env
    return concat(fn({}, 0, tostring, html_escape))
  end
end

local page = [[
<!DOCTYPE html>
<html><head><title><%= title %></title></head>
<body>
<h1><%= title %></h1>
<% if user then -%>
<p class="welcome">Hello, <%= user.name %> (<%= user.role %>)!</p>
<% end -%>
<ul>
<% for i, item in ipairs(items) do -%>
  <li class="<%= i % 2 == 0 and "even" or "odd" %>"><a href="/item/<%= item.id %>"><%= item.label %></a>
    <span><%- string.format("%.2f", item.price) %></span>
    <% if item.tags then %><% for _, t in ipairs(item.tags) do %><em><%= t %></em><% end %><% end %>
  </li>
<% end -%>
</ul>
<p>Total: <%- total %></p>
</body></html>
]]

local N = tonumber(arg and arg[1]) or 8000

local items = {}
for i = 1, 40 do
  local tags
  if i % 3 == 0 then tags = { "new", "<hot>", "sale & more" } end
  items[i] = { id = i, label = "Item #" .. i .. " <b>" .. string.rep("x", i % 7) .. "</b>", price = i * 1.25, tags = tags }
end

local parser = Parser.new()
local h, total_len = 0, 0
-- uncached: compile + load per render (the common way etlua is misused)
for r = 1, N / 10 do
  local tpl = assert(parser:compile(page))
  local out = tpl({ title = "Shop & Co " .. r, user = (r % 2 == 0) and { name = "Ann <" .. r .. ">", role = "admin" } or nil,
                    items = items, total = #items })
  total_len = total_len + #out
end
local tpl = assert(parser:compile_cached(page))
local out
for r = 1, N do
  out = tpl({ title = "Shop & Co " .. r, user = (r % 2 == 0) and { name = "Ann <" .. r .. ">", role = "admin" } or nil,
              items = items, total = #items })
  total_len = total_len + #out
  for i = 1, #out, 13 do h = (h * 31 + out:byte(i)) % 1000000007 end
end
print("len", total_len, "hash", h)
print(out:sub(1, 300))
