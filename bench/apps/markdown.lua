-- markdown: converts generated markdown text to HTML (headings, paragraphs, bullet and numbered lists, fenced code
-- blocks, block quotes, inline *em*, **strong**, `code`, [links](url) and HTML escaping) with line-wise state
-- handling and string.gsub / find / format, output assembled in a buffer table.  String-builder / pattern heavy.
-- Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
-- Output: html length, counts of tags, and a polynomial hash of every 7th output byte.
local ROUNDS = tonumber(arg and arg[1]) or 30
local gsub, find, match, sub, byte, format, rep = string.gsub, string.find, string.match, string.sub, string.byte, string.format, string.rep
local concat, insert = table.concat, table.insert

-- ---- input generator ------------------------------------------------------------------------------------
local seed = 99
local function rnd(n) seed = (seed * 16807) % 2147483647 return seed % n + 1 end
local WORDS = { "lorem", "ipsum", "dolor", "sit", "amet", "consectetur", "adipiscing", "elit", "sed", "do", "eiusmod",
  "tempor", "incididunt", "ut", "labore", "et", "dolore", "magna", "aliqua", "enim", "minim", "veniam", "quis",
  "nostrud", "exercitation", "ullamco", "laboris", "nisi", "aliquip", "ex", "ea", "commodo" }
local function words(n)
  local t = {}
  for i = 1, n do
    local w = WORDS[rnd(#WORDS)]
    local r = rnd(20)
    if r == 1 then w = "*" .. w .. "*"
    elseif r == 2 then w = "**" .. w .. "**"
    elseif r == 3 then w = "`" .. w .. "(" .. rnd(9) .. ")`"
    elseif r == 4 then w = "[" .. w .. "](http://example.org/" .. w .. "?a=1&b=" .. rnd(99) .. ")"
    elseif r == 5 then w = "<" .. w .. ">"
    end
    t[i] = w
  end
  return concat(t, " ")
end
local function genDoc()
  local out = {}
  for sec = 1, 30 do
    out[#out + 1] = rep("#", rnd(3)) .. " Section " .. sec .. " " .. words(3)
    out[#out + 1] = ""
    for p = 1, 3 do
      out[#out + 1] = words(20 + rnd(20))
      out[#out + 1] = words(10 + rnd(10))
      out[#out + 1] = ""
    end
    for l = 1, rnd(5) + 2 do out[#out + 1] = "- " .. words(5 + rnd(6)) end
    out[#out + 1] = ""
    for l = 1, rnd(4) + 1 do out[#out + 1] = l .. ". " .. words(4 + rnd(6)) end
    out[#out + 1] = ""
    out[#out + 1] = "> " .. words(8 + rnd(8))
    out[#out + 1] = ""
    out[#out + 1] = "```"
    for l = 1, 3 do out[#out + 1] = "local x" .. l .. " = a < b and c > d or &e" end
    out[#out + 1] = "```"
    out[#out + 1] = ""
  end
  return concat(out, "\n")
end

-- ---- converter -------------------------------------------------------------------------------------------
local ESC = { ["&"] = "&amp;", ["<"] = "&lt;", [">"] = "&gt;" }
local function escape(s) return (gsub(s, "[&<>]", ESC)) end
local tags = { a = 0, em = 0, strong = 0, code = 0 }
local function inline(s)
  s = escape(s)
  s = gsub(s, "`([^`]+)`", function(c) tags.code = tags.code + 1 return "<code>" .. c .. "</code>" end)
  s = gsub(s, "%*%*([^%*]+)%*%*", function(c) tags.strong = tags.strong + 1 return "<strong>" .. c .. "</strong>" end)
  s = gsub(s, "%*([^%*]+)%*", function(c) tags.em = tags.em + 1 return "<em>" .. c .. "</em>" end)
  s = gsub(s, "%[([^%]]+)%]%(([^%)]+)%)", function(text, url)
    tags.a = tags.a + 1
    return format('<a href="%s">%s</a>', url, text)
  end)
  return s
end

local function convert(text)
  local out = {}
  local state, para = nil, {}
  local function flushPara()
    if #para > 0 then
      insert(out, "<p>" .. inline(concat(para, " ")) .. "</p>")
      para = {}
    end
  end
  local function close()
    if state == "ul" then insert(out, "</ul>") elseif state == "ol" then insert(out, "</ol>") elseif state == "quote" then insert(out, "</blockquote>") end
    state = nil
  end
  local inCode = false
  for line in (text .. "\n"):gmatch("([^\n]*)\n") do
    if inCode then
      if line == "```" then insert(out, "</pre>") inCode = false else insert(out, escape(line)) end
    elseif line == "```" then
      flushPara(); close()
      insert(out, "<pre>")
      inCode = true
    elseif line == "" then
      flushPara(); close()
    else
      local hashes, title = match(line, "^(#+) (.*)$")
      if hashes then
        flushPara(); close()
        insert(out, format("<h%d>%s</h%d>", #hashes, inline(title), #hashes))
      else
        local item = match(line, "^%- (.*)$")
        local num, nitem = match(line, "^(%d+)%. (.*)$")
        local quote = match(line, "^> (.*)$")
        if item then
          flushPara()
          if state ~= "ul" then close(); insert(out, "<ul>"); state = "ul" end
          insert(out, "<li>" .. inline(item) .. "</li>")
        elseif num then
          flushPara()
          if state ~= "ol" then close(); insert(out, "<ol>"); state = "ol" end
          insert(out, "<li>" .. inline(nitem) .. "</li>")
        elseif quote then
          flushPara()
          if state ~= "quote" then close(); insert(out, "<blockquote>"); state = "quote" end
          insert(out, "<p>" .. inline(quote) .. "</p>")
        else
          insert(para, line)
        end
      end
    end
  end
  flushPara(); close()
  return concat(out, "\n")
end

local doc = genDoc()
local html
for r = 1, ROUNDS do html = convert(doc) end
local h = 0
for i = 1, #html, 7 do h = (h * 31 + byte(html, i)) % 1000003 end
print("markdown", #doc, #html, tags.a, tags.em, tags.strong, tags.code, h)
