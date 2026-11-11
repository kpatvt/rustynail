-- A Lua-in-Lua lexer (in the style of the lexers of luacheck / LuaMinify / penlight's pl.lexer): byte-level dispatch,
-- long strings/comments, numbers, escapes; then a token-stream "minifier" and statistics over generated source.
local sbyte, ssub, sfind, schar = string.byte, string.sub, string.find, string.char

local keywords = {}
for w in ("and break do else elseif end false for function goto if in local nil not or repeat return then true until while"):gmatch("%a+") do
  keywords[w] = true
end

local BYTE_0, BYTE_9 = sbyte("0"), sbyte("9")
local BYTE_a, BYTE_z, BYTE_A, BYTE_Z, BYTE_UNDER = sbyte("a"), sbyte("z"), sbyte("A"), sbyte("Z"), sbyte("_")
local BYTE_NL, BYTE_CR, BYTE_SPACE, BYTE_TAB = 10, 13, 32, 9
local BYTE_DASH, BYTE_LBRACKET, BYTE_EQ, BYTE_QUOTE, BYTE_DQUOTE, BYTE_DOT, BYTE_BSLASH = 45, 91, 61, 39, 34, 46, 92

local function is_alpha(b) return (b >= BYTE_a and b <= BYTE_z) or (b >= BYTE_A and b <= BYTE_Z) or b == BYTE_UNDER end
local function is_digit(b) return b >= BYTE_0 and b <= BYTE_9 end
local function is_alnum(b) return is_alpha(b) or is_digit(b) end

local Lexer = {}
Lexer.__index = Lexer

function Lexer.new(src)
  return setmetatable({ src = src, pos = 1, line = 1 }, Lexer)
end

function Lexer:peek(off) return sbyte(self.src, self.pos + (off or 0)) end

function Lexer:skip_long_bracket()
  -- at '[', returns level or nil
  local p = self.pos + 1
  local level = 0
  while sbyte(self.src, p) == BYTE_EQ do level = level + 1; p = p + 1 end
  if sbyte(self.src, p) == BYTE_LBRACKET then return level, p + 1 end
  return nil
end

function Lexer:read_long(level, start)
  local close = "]" .. string.rep("=", level) .. "]"
  local s, e = sfind(self.src, close, start, true)
  if not s then error("unfinished long string at line " .. self.line) end
  local body = ssub(self.src, start, s - 1)
  for _ in body:gmatch("\n") do self.line = self.line + 1 end
  self.pos = e + 1
  return body
end

local escapes = { n = "\n", t = "\t", r = "\r", a = "\a", b = "\b", f = "\f", v = "\v", ["\\"] = "\\", ['"'] = '"', ["'"] = "'", ["\n"] = "\n" }

function Lexer:read_string(q)
  local buf, n = {}, 0
  local p = self.pos + 1
  local src = self.src
  while true do
    local b = sbyte(src, p)
    if not b then error("unfinished string") end
    if b == q then break end
    if b == BYTE_BSLASH then
      local c = ssub(src, p + 1, p + 1)
      if escapes[c] then
        n = n + 1; buf[n] = escapes[c]; p = p + 2
      elseif c:match("%d") then
        local d = src:match("^%d%d?%d?", p + 1)
        n = n + 1; buf[n] = schar(tonumber(d)); p = p + 1 + #d
      elseif c == "x" then
        n = n + 1; buf[n] = schar(tonumber(ssub(src, p + 2, p + 3), 16)); p = p + 4
      else
        error("bad escape \\" .. c)
      end
    else
      n = n + 1; buf[n] = schar(b); p = p + 1
    end
  end
  self.pos = p + 1
  return table.concat(buf)
end

local ops3 = { ["..."] = true }
local ops2 = { ["=="] = true, ["~="] = true, ["<="] = true, [">="] = true, [".."] = true, ["::"] = true, ["//"] = true, ["<<"] = true, [">>"] = true }

function Lexer:next()
  local src = self.src
  while true do
    local b = sbyte(src, self.pos)
    if not b then return "<eof>" end
    if b == BYTE_NL then
      self.line = self.line + 1; self.pos = self.pos + 1
    elseif b == BYTE_SPACE or b == BYTE_TAB or b == BYTE_CR then
      self.pos = self.pos + 1
    elseif b == BYTE_DASH and sbyte(src, self.pos + 1) == BYTE_DASH then
      self.pos = self.pos + 2
      local level, start
      if sbyte(src, self.pos) == BYTE_LBRACKET then level, start = self:skip_long_bracket() end
      if level then
        self:read_long(level, start)
      else
        local e = sfind(src, "\n", self.pos, true) or #src + 1
        self.pos = e
      end
    else
      break
    end
  end
  local b = sbyte(src, self.pos)
  local start = self.pos
  if is_alpha(b) then
    local p = self.pos + 1
    while true do
      local c = sbyte(src, p)
      if not c or not is_alnum(c) then break end
      p = p + 1
    end
    local word = ssub(src, start, p - 1)
    self.pos = p
    if keywords[word] then return "keyword", word end
    return "name", word
  elseif is_digit(b) or (b == BYTE_DOT and is_digit(sbyte(src, self.pos + 1) or 0)) then
    local s, e = sfind(src, "^0[xX][%x%.]+[pP]?[%+%-]?%x*", start)
    if not s then s, e = sfind(src, "^%d*%.?%d*[eE][%+%-]?%d+", start) end
    if not s then s, e = sfind(src, "^%d*%.?%d*", start) end
    self.pos = e + 1
    return "number", ssub(src, s, e)
  elseif b == BYTE_QUOTE or b == BYTE_DQUOTE then
    return "string", self:read_string(b)
  elseif b == BYTE_LBRACKET then
    local level, s2 = self:skip_long_bracket()
    if level then return "string", self:read_long(level, s2) end
    self.pos = self.pos + 1
    return "op", "["
  else
    local t3 = ssub(src, start, start + 2)
    if ops3[t3] then self.pos = start + 3; return "op", t3 end
    local t2 = ssub(src, start, start + 1)
    if ops2[t2] then self.pos = start + 2; return "op", t2 end
    self.pos = start + 1
    return "op", schar(b)
  end
end

-- generate a deterministic synthetic Lua source file
local function gen_source(nfuncs)
  local parts = {}
  for i = 1, nfuncs do
    parts[#parts + 1] = string.format([===[
-- function number %d: computes things
local function f_%d(a, b, ...)
  local t = { x = %d, y = 0x%X, s = "str\t%d\n", ["key %d"] = 'single\'s', 3.25e-2, 1e10 }
  --[==[ long comment
  with ]] inside ]==]
  if a >= b and not (a ~= %d) or #t > 2 then
    for k = 1, %d do t[k] = (t[k] or 0) + k * a // 2 .. "" end
  elseif a <= b then
    return [[long
string %d]], ...
  end
  while b < a do b = b + 1; a = a - 1 end
  return t.x + t.y, select('#', ...)
end
]===], i, i, i, i * 31, i, i, i, i % 9, i)
  end
  return table.concat(parts)
end

local N = tonumber(arg and arg[1]) or 50
local src = gen_source(300)
local counts = {}
local total_tokens, lines = 0, 0
local min_parts
for r = 1, N do
  local lx = Lexer.new(src)
  local out, on = {}, 0
  local prev = "op"
  while true do
    local kind, val = lx:next()
    if kind == "<eof>" then break end
    total_tokens = total_tokens + 1
    counts[kind] = (counts[kind] or 0) + 1
    -- minifier: re-emit with minimal spacing
    if kind == "string" then
      val = string.format("%q", val)
    end
    if (kind == "name" or kind == "keyword" or kind == "number") and (prev == "name" or prev == "keyword" or prev == "number") then
      on = on + 1; out[on] = " "
    end
    on = on + 1; out[on] = val
    prev = kind
  end
  lines = lines + lx.line
  min_parts = table.concat(out)
end
local kinds = {}
for k in pairs(counts) do kinds[#kinds + 1] = k end
table.sort(kinds)
local s = {}
for _, k in ipairs(kinds) do s[#s + 1] = k .. "=" .. counts[k] end
print("tokens", total_tokens, "lines", lines, table.concat(s, " "))
print("minified", #src, "->", #min_parts)
print(min_parts:sub(1, 200))
