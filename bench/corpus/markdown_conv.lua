-- A markdown.lua-style converter (Niklas Frykholm's markdown.lua structure: protect/unprotect with hashes,
-- block splitting, lists, code blocks, blockquotes, headers, emphasis, links, auto-escaping via gsub).
local M = {}

local HASH = { inited = false, identifier = "", counter = 0, table = {} }
local function init_hash(text)
  HASH.inited = true
  HASH.identifier = ""
  HASH.counter = 0
  HASH.table = {}
  local s = "HASH"
  local counter = 10
  while text:find(s, 1, true) do
    s = s .. string.char(65 + counter % 26)
    counter = counter + 1
  end
  HASH.identifier = s
end
local function hash(s)
  assert(HASH.inited)
  if not HASH.table[s] then
    HASH.counter = HASH.counter + 1
    local id = HASH.identifier .. HASH.counter .. "X"
    HASH.table[s] = id
    HASH.table[id] = s
  end
  return HASH.table[s]
end
local function unhash(text)
  local found = true
  while found do
    found = false
    text = text:gsub(HASH.identifier .. "%d+X", function(id)
      local s = HASH.table[id]
      if s then found = true; return s end
    end)
  end
  return text
end

local function escape_special(s)
  return (s:gsub("&", "&amp;"):gsub("<", "&lt;"):gsub(">", "&gt;"))
end

local escape_chars = "'\\`*_{}[]()>#+-.!'"
local escape_table = {}
for i = 1, #escape_chars do
  local c = escape_chars:sub(i, i)
  escape_table[c] = "\001" .. string.byte(c) .. "\002"
end
local function protect_escapes(text)
  return (text:gsub("\\(.)", function(c) return escape_table[c] or ("\\" .. c) end))
end
local function unprotect_escapes(text)
  return (text:gsub("\001(%d+)\002", function(n) return string.char(tonumber(n)) end))
end

-- span level
local function code_spans(s)
  return (s:gsub("(`+)(.-)%1", function(_, code)
    return hash("<code>" .. escape_special(code:gsub("^%s+", ""):gsub("%s+$", "")) .. "</code>")
  end))
end
local function links(s, refs)
  s = s:gsub("!%[(.-)%]%((.-)%)", function(alt, url) return hash('<img src="' .. url .. '" alt="' .. alt .. '"/>') end)
  s = s:gsub("%[(.-)%]%((.-)%)", function(text, url)
    local u, title = url:match('^(%S+)%s+"(.-)"$')
    if u then return hash('<a href="' .. u .. '" title="' .. title .. '">') .. text .. hash("</a>") end
    return hash('<a href="' .. url .. '">') .. text .. hash("</a>")
  end)
  s = s:gsub("%[(.-)%]%[(.-)%]", function(text, id)
    if id == "" then id = text end
    local ref = refs[id:lower()]
    if not ref then return nil end
    return hash('<a href="' .. ref .. '">') .. text .. hash("</a>")
  end)
  s = s:gsub("<(https?://[^>]+)>", function(url) return hash('<a href="' .. url .. '">' .. url .. "</a>") end)
  return s
end
local function emphasis(s)
  s = s:gsub("%*%*(%S.-%S?)%*%*", "<strong>%1</strong>")
  s = s:gsub("__(%S.-%S?)__", "<strong>%1</strong>")
  s = s:gsub("%*([^%s%*][^%*]-)%*", "<em>%1</em>")
  s = s:gsub("_([^%s_][^_]-)_", "<em>%1</em>")
  return s
end
local function span(s, refs)
  s = code_spans(s)
  s = links(s, refs)
  s = s:gsub("&(%w+);", function(e) return hash("&" .. e .. ";") end)
  s = escape_special(s)
  s = emphasis(s)
  s = s:gsub("  +\n", hash("<br/>") .. "\n")
  return s
end

-- block level
local function split_lines(text)
  local lines = {}
  for line in (text .. "\n"):gmatch("(.-)\n") do lines[#lines + 1] = line end
  return lines
end

local function classify(line)
  if line:match("^%s*$") then return "empty" end
  if line:match("^#+%s") then return "header" end
  if line:match("^    ") or line:match("^\t") then return "code" end
  if line:match("^>") then return "quote" end
  if line:match("^%s*[%*%-%+]%s+") then return "ul" end
  if line:match("^%s*%d+%.%s+") then return "ol" end
  if line:match("^%-%-%-+%s*$") or line:match("^%*%s*%*%s*%*[%*%s]*$") then return "rule" end
  return "normal"
end

local blocks
blocks = function(lines, refs)
  local out = {}
  local i = 1
  local n = #lines
  while i <= n do
    local line = lines[i]
    local kind = classify(line)
    if kind == "empty" then
      i = i + 1
    elseif kind == "header" then
      local hashes, text = line:match("^(#+)%s+(.-)%s*#*$")
      local lvl = #hashes
      out[#out + 1] = string.format("<h%d>%s</h%d>", lvl, span(text, refs), lvl)
      i = i + 1
    elseif kind == "rule" then
      out[#out + 1] = "<hr/>"
      i = i + 1
    elseif kind == "code" then
      local code = {}
      while i <= n and (classify(lines[i]) == "code" or (classify(lines[i]) == "empty" and lines[i + 1] and classify(lines[i + 1]) == "code")) do
        code[#code + 1] = escape_special((lines[i]:gsub("^    ", ""):gsub("^\t", "")))
        i = i + 1
      end
      out[#out + 1] = hash("<pre><code>" .. table.concat(code, "\n") .. "\n</code></pre>")
    elseif kind == "quote" then
      local q = {}
      while i <= n and classify(lines[i]) ~= "empty" do
        q[#q + 1] = (lines[i]:gsub("^>%s?", ""))
        i = i + 1
      end
      out[#out + 1] = "<blockquote>\n" .. blocks(q, refs) .. "\n</blockquote>"
    elseif kind == "ul" or kind == "ol" then
      local tag = kind
      local items = {}
      local cur
      while i <= n do
        local l = lines[i]
        local k = classify(l)
        if k == tag then
          cur = { (l:gsub("^%s*[%*%-%+]%s+", ""):gsub("^%s*%d+%.%s+", "")) }
          items[#items + 1] = cur
          i = i + 1
        elseif k == "empty" then
          if lines[i + 1] and (classify(lines[i + 1]) == tag or lines[i + 1]:match("^    ")) then i = i + 1 else break end
        elseif l:match("^    ") or k == "normal" then
          cur[#cur + 1] = (l:gsub("^    ", ""))
          i = i + 1
        else
          break
        end
      end
      local parts = { "<" .. tag .. ">" }
      for _, it in ipairs(items) do
        if #it > 1 and classify(it[2]) ~= "normal" then
          parts[#parts + 1] = "<li>" .. span(it[1], refs) .. "\n" .. blocks({ table.unpack and table.unpack(it, 2) or unpack(it, 2) }, refs) .. "</li>"
        else
          parts[#parts + 1] = "<li>" .. span(table.concat(it, "\n"), refs) .. "</li>"
        end
      end
      parts[#parts + 1] = "</" .. tag .. ">"
      out[#out + 1] = table.concat(parts, "\n")
    else
      local para = {}
      while i <= n and classify(lines[i]) == "normal" do
        para[#para + 1] = lines[i]
        i = i + 1
      end
      local text = table.concat(para, "\n")
      local setext = lines[i] and lines[i]:match("^([=%-])%1%1+%s*$")
      if setext and #para == 1 then
        local lvl = setext == "=" and 1 or 2
        out[#out + 1] = string.format("<h%d>%s</h%d>", lvl, span(text, refs), lvl)
        i = i + 1
      else
        out[#out + 1] = "<p>" .. span(text, refs) .. "</p>"
      end
    end
  end
  return table.concat(out, "\n\n")
end

function M.markdown(text)
  init_hash(text)
  text = text:gsub("\r\n", "\n"):gsub("\r", "\n")
  text = protect_escapes(text)
  local refs = {}
  text = text:gsub("\n%s*%[([^%]]+)%]:%s*(%S+)[^\n]*", function(id, url)
    refs[id:lower()] = url
    return ""
  end)
  local lines = split_lines(text)
  local html = blocks(lines, refs)
  html = unhash(html)
  html = unprotect_escapes(html)
  return html
end

-- Workload
local doc_parts = {}
for i = 1, 25 do
  doc_parts[#doc_parts + 1] = string.format([[
Section %d
=========

This is *paragraph* number %d with **bold text**, `inline <code>`, a [link](http://example.com/%d "Title %d")
and a reference [ref link][r%d]. Escaped \*stars\* and an & ampersand < less.
A second line with _emphasis_ and __strong__ and trailing spaces
next line.

## Subheading %d ##

* item one with `code`
* item two with [a link](/x/%d)
* item three
    continuation of item three

1. first
2. second **bold**
3. third

> quoted text %d
> more *quote*

    local x = %d
    print(x < 10 and "a" or "b")

---

![image %d](/img/%d.png)
<http://auto.link/%d>

[r%d]: http://ref.example.com/%d
]], i, i, i, i, i, i, i, i, i, i, i, i, i, i)
end
local doc = table.concat(doc_parts, "\n")

local N = tonumber(arg and arg[1]) or 300
local html, total, h = nil, 0, 0
for r = 1, N do
  html = M.markdown(doc)
  total = total + #html
end
for i = 1, #html, 3 do h = (h * 31 + html:byte(i)) % 1000000007 end
print("total", total, "hash", h)
print(html:sub(1, 900))
