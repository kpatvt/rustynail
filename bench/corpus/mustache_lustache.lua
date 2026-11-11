-- Derived from lustache (https://github.com/Olivine-Labs/lustache), Copyright (c) Olivine Labs, LLC, MIT License; see THIRD_PARTY.md. Mustache renderer: scanner, token tree (sections, inverted, partials), context stack lookup.
local string_find, string_sub, string_gsub, string_match = string.find, string.sub, string.gsub, string.match
local table_concat, table_insert, table_remove = table.concat, table.insert, table.remove

local html_escape_characters = {
  ["&"] = "&amp;", ["<"] = "&lt;", [">"] = "&gt;", ['"'] = "&quot;", ["'"] = "&#39;", ["/"] = "&#x2F;",
}

local function is_array(array)
  if type(array) ~= "table" then return false end
  local max, n = 0, 0
  for k, _ in pairs(array) do
    if not (type(k) == "number" and k > 0 and math.floor(k) == k) then
      return false
    end
    max = math.max(max, k)
    n = n + 1
  end
  return n == max
end

local function escape_tags(tags)
  return {
    string_gsub(tags[1], "%%", "%%%%") .. "%s*",
    "%s*" .. string_gsub(tags[2], "%%", "%%%%"),
  }
end

-- Scanner
local Scanner = {}
Scanner.__index = Scanner
function Scanner.new(str)
  return setmetatable({ str = str, tail = str, pos = 1 }, Scanner)
end
function Scanner:eos() return self.tail == "" end
function Scanner:scan(pattern)
  local match = string_match(self.tail, pattern)
  if match and string_find(self.tail, pattern) == 1 then
    self.tail = string_sub(self.tail, #match + 1)
    self.pos = self.pos + #match
    return match
  end
end
function Scanner:scan_until(pattern)
  local match
  local pos = string_find(self.tail, pattern)
  if pos == nil then
    match = self.tail
    self.pos = self.pos + #self.tail
    self.tail = ""
  elseif pos == 1 then
    match = nil
  else
    match = string_sub(self.tail, 1, pos - 1)
    self.tail = string_sub(self.tail, pos)
    self.pos = self.pos + #match
  end
  return match
end

-- Context
local Context = {}
Context.__index = Context
function Context.new(view, parent)
  return setmetatable({ view = view, parent = parent, cache = {} }, Context)
end
function Context:push(view) return Context.new(view, self) end
function Context:lookup(name)
  local value = self.cache[name]
  if not value then
    if name == "." then
      value = self.view
    else
      local context = self
      while context do
        if string_find(name, ".", 1, true) then
          value = context.view
          for part in name:gmatch("[^%.]+") do
            if type(value) ~= "table" then value = nil break end
            value = value[part]
          end
        else
          value = type(context.view) == "table" and context.view[name]
        end
        if value ~= nil and value ~= false then break end
        context = context.parent
      end
    end
    self.cache[name] = value
  end
  return value
end

-- Renderer
local patterns = { white = "%s*", space = "%s+", nonSpace = "%S", eq = "%s*=", curly = "%s*}", tag = "[#\\^/>{&=!]" }

local function nest_tokens(tokens)
  local tree, collector, sections = {}, nil, {}
  collector = tree
  for _, token in ipairs(tokens) do
    if token.type == "#" or token.type == "^" then
      token.tokens = {}
      sections[#sections + 1] = token
      collector[#collector + 1] = token
      collector = token.tokens
    elseif token.type == "/" then
      if #sections == 0 then error("Unopened section: " .. token.value) end
      local section = table_remove(sections)
      if section.value ~= token.value then error("Unclosed section: " .. section.value) end
      section.closingTagIndex = token.startIndex
      if #sections > 0 then collector = sections[#sections].tokens else collector = tree end
    else
      collector[#collector + 1] = token
    end
  end
  if #sections > 0 then error("Unclosed section: " .. sections[#sections].value) end
  return tree
end

local function squash_tokens(tokens)
  local out, txt = {}, {}
  local lastToken
  for _, token in ipairs(tokens) do
    if token.type == "text" then
      if lastToken and lastToken.type == "text" then
        lastToken.value = lastToken.value .. token.value
        lastToken.endIndex = token.endIndex
      else
        lastToken = token
        out[#out + 1] = token
      end
    else
      lastToken = token
      out[#out + 1] = token
    end
  end
  return out
end

local function parse(template, tags)
  tags = tags or { "{{", "}}" }
  local tag_patterns = escape_tags(tags)
  local scanner = Scanner.new(template)
  local tokens = {}
  while not scanner:eos() do
    local start = scanner.pos
    local value = scanner:scan_until(tag_patterns[1])
    if value then
      tokens[#tokens + 1] = { type = "text", value = value, startIndex = start, endIndex = scanner.pos - 1 }
    end
    if not scanner:scan(tag_patterns[1]) then break end
    local typ = scanner:scan(patterns.tag) or "name"
    scanner:scan(patterns.white)
    if typ == "=" then
      value = scanner:scan_until(patterns.eq)
      scanner:scan(patterns.eq)
      scanner:scan_until(tag_patterns[2])
    elseif typ == "{" then
      value = scanner:scan_until("%s*}" .. string_sub(tag_patterns[2], 4))
      scanner:scan(patterns.curly)
      scanner:scan_until(tag_patterns[2])
      typ = "&"
    else
      value = scanner:scan_until(tag_patterns[2])
    end
    if not scanner:scan(tag_patterns[2]) then error("Unclosed tag at " .. scanner.pos) end
    tokens[#tokens + 1] = { type = typ, value = value, startIndex = start, endIndex = scanner.pos - 1 }
  end
  return nest_tokens(squash_tokens(tokens))
end

local Renderer = {}
Renderer.__index = Renderer
function Renderer.new() return setmetatable({ cache = {}, partial_cache = {} }, Renderer) end

local function escape(s)
  return (string_gsub(s, '[&<>"\'/]', html_escape_characters))
end

function Renderer:render_tokens(tokens, context, partials, originalTemplate)
  local buffer = {}
  for i, token in ipairs(tokens) do
    local t = token.type
    if t == "#" then
      local value = context:lookup(token.value)
      if type(value) == "table" then
        if is_array(value) then
          for _, v in ipairs(value) do
            buffer[#buffer + 1] = self:render_tokens(token.tokens, context:push(v), partials, originalTemplate)
          end
        else
          buffer[#buffer + 1] = self:render_tokens(token.tokens, context:push(value), partials, originalTemplate)
        end
      elseif type(value) == "function" then
        local section_text = string_sub(originalTemplate, token.endIndex + 1, token.closingTagIndex - 1)
        buffer[#buffer + 1] = value(section_text, function(s) return self:render(s, context.view, partials) end)
      elseif value then
        buffer[#buffer + 1] = self:render_tokens(token.tokens, context, partials, originalTemplate)
      end
    elseif t == "^" then
      local value = context:lookup(token.value)
      if not value or (type(value) == "table" and is_array(value) and #value == 0) then
        buffer[#buffer + 1] = self:render_tokens(token.tokens, context, partials, originalTemplate)
      end
    elseif t == ">" then
      local partial = partials and partials[token.value]
      if partial then
        local ptoks = self.partial_cache[partial]
        if not ptoks then ptoks = parse(partial); self.partial_cache[partial] = ptoks end
        buffer[#buffer + 1] = self:render_tokens(ptoks, context, partials, partial)
      end
    elseif t == "&" then
      local value = context:lookup(token.value)
      if value ~= nil then buffer[#buffer + 1] = tostring(value) end
    elseif t == "name" then
      local value = context:lookup(token.value)
      if value ~= nil then buffer[#buffer + 1] = escape(tostring(value)) end
    elseif t == "text" then
      buffer[#buffer + 1] = token.value
    end
  end
  return table_concat(buffer)
end

function Renderer:render(template, view, partials)
  local tokens = self.cache[template]
  if not tokens then
    tokens = parse(template)
    self.cache[template] = tokens
  end
  return self:render_tokens(tokens, Context.new(view), partials, template)
end

-- Workload
local template = [[
<h1>{{title}}</h1>
{{#user}}<p>Welcome back, {{name}}! You have {{messages.count}} new messages.</p>{{/user}}
{{^user}}<p>Please log in.</p>{{/user}}
<table>
{{#rows}}
  <tr class="{{#odd}}odd{{/odd}}{{^odd}}even{{/odd}}">
    <td>{{id}}</td><td>{{name}}</td><td>{{{html}}}</td><td>{{price}}</td>
    {{#tags}}<span>{{.}}</span>{{/tags}}
    {{> footer}}
  </tr>
{{/rows}}
</table>
{{#bold}}some <b>{{title}}</b> text{{/bold}}
]]
local partials = { footer = "<td class=\"f\">{{name}} / {{title}}</td>" }

local N = tonumber(arg and arg[1]) or 4000
local rows = {}
for i = 1, 30 do
  rows[i] = { id = i, name = "row<" .. i .. ">", html = "<i>" .. i .. "</i>", price = string.format("%.2f", i * 3.5),
              odd = i % 2 == 1, tags = (i % 4 == 0) and { "a", "b&c", "d" } or {} }
end
local r = Renderer.new()
local h, len, out = 0, 0, nil
for k = 1, N do
  local view = { title = "Report " .. k, rows = rows,
                 user = (k % 3 ~= 0) and { name = "Bob'" .. k, messages = { count = k % 17 } } or false,
                 bold = function(text, render) return "<strong>" .. render(text) .. "</strong>" end }
  out = r:render(template, view, partials)
  len = len + #out
  for i = 1, #out, 11 do h = (h * 31 + out:byte(i)) % 1000000007 end
  if k % 50 == 0 then r.cache = {} end -- re-parse sometimes
end
print("len", len, "hash", h)
print(out:sub(1, 400))
