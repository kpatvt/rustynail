-- Json: hand-written recursive-descent JSON parser over a generated document (port from the public
-- description of the are-we-fast-yet "Json" benchmark, a minimal-json style parser: one character at a time via
-- string.sub, objects as name/value arrays, strings assembled in a buffer table) plus a serializer.
-- Plain Lua 5.1-compatible; runs unchanged on PUC 5.4/5.5 and LuaJIT.
-- Output: node count, sum of all numbers, total string length, serialized length (same every iteration).
local N = tonumber(arg and arg[1]) or 35
local sub, concat, tonumber, tostring = string.sub, table.concat, tonumber, tostring
local insert = table.insert

-- Build the document text once (deterministic).
local function buildDocument()
  local parts = {}
  local seed = 74755
  local function rnd(n) seed = (seed * 1309 + 13849) % 65536 return seed % n end
  parts[#parts + 1] = '{"head":{"requestCounter":4,"name":"bench \\"doc\\"","tags":["a","b","c"]},"operations":['
  for i = 1, 40 do
    if i > 1 then parts[#parts + 1] = "," end
    parts[#parts + 1] = string.format(
      '{"type":"update","id":%d,"ok":%s,"ratio":%d.%d,"neg":-%d,"exp":%de-2,"payload":{"title":"item number %d",' ..
      '"values":[%d,%d,%d,%d],"nested":{"x":%d,"y":null,"flag":%s,"text":"line1\\nline2 \\u0041\\t"}}}',
      i, (rnd(2) == 0) and "true" or "false", rnd(100), rnd(1000), rnd(500) + 1, rnd(50), i,
      rnd(1000), rnd(1000), rnd(1000), rnd(1000), rnd(10000), (rnd(2) == 0) and "true" or "false")
  end
  parts[#parts + 1] = "]}"
  return concat(parts)
end

local NULL = {}  -- singleton for json null
local Parser = {}
Parser.__index = Parser
local function newParser(text)
  local p = setmetatable({ text = text, index = 0, cur = nil, captureBuf = nil }, Parser)
  return p
end
function Parser:read()
  self.index = self.index + 1
  local c = sub(self.text, self.index, self.index)
  if c == "" then self.cur = nil else self.cur = c end
end
function Parser:error(msg) error("parse error at " .. self.index .. ": " .. msg) end
function Parser:skipWhiteSpace()
  local c = self.cur
  while c == " " or c == "\t" or c == "\n" or c == "\r" do
    self:read()
    c = self.cur
  end
end
function Parser:readChar(ch)
  if self.cur ~= ch then self:error("expected " .. ch) end
  self:read()
end
function Parser:readValue()
  local c = self.cur
  if c == "n" then return self:readNull()
  elseif c == "t" then return self:readTrue()
  elseif c == "f" then return self:readFalse()
  elseif c == '"' then return self:readString()
  elseif c == "[" then return self:readArray()
  elseif c == "{" then return self:readObject()
  elseif c == "-" or (c ~= nil and c >= "0" and c <= "9") then return self:readNumber()
  end
  self:error("value expected")
end
function Parser:readArray()
  self:read()
  local arr = { kind = "array", items = {} }
  self:skipWhiteSpace()
  if self.cur == "]" then self:read() return arr end
  while true do
    self:skipWhiteSpace()
    insert(arr.items, self:readValue())
    self:skipWhiteSpace()
    if self.cur == "," then self:read() elseif self.cur == "]" then self:read() return arr else self:error("expected , or ]") end
  end
end
function Parser:readObject()
  self:read()
  local obj = { kind = "object", names = {}, values = {} }
  self:skipWhiteSpace()
  if self.cur == "}" then self:read() return obj end
  while true do
    self:skipWhiteSpace()
    local name = self:readString().value
    self:skipWhiteSpace()
    self:readChar(":")
    self:skipWhiteSpace()
    insert(obj.names, name)
    insert(obj.values, self:readValue())
    self:skipWhiteSpace()
    if self.cur == "," then self:read() elseif self.cur == "}" then self:read() return obj else self:error("expected , or }") end
  end
end
function Parser:readNull() self:read(); self:readChar("u"); self:readChar("l"); self:readChar("l") return NULL end
function Parser:readTrue() self:read(); self:readChar("r"); self:readChar("u"); self:readChar("e") return { kind = "bool", value = true } end
function Parser:readFalse() self:read(); self:readChar("a"); self:readChar("l"); self:readChar("s"); self:readChar("e") return { kind = "bool", value = false } end
function Parser:readString()
  self:readChar('"')
  local buf = {}
  while self.cur ~= '"' do
    local c = self.cur
    if c == nil then self:error("unterminated string") end
    if c == "\\" then
      self:read()
      c = self.cur
      if c == "n" then insert(buf, "\n")
      elseif c == "t" then insert(buf, "\t")
      elseif c == "r" then insert(buf, "\r")
      elseif c == "b" then insert(buf, "\b")
      elseif c == "f" then insert(buf, "\f")
      elseif c == "u" then
        local hex = sub(self.text, self.index + 1, self.index + 4)
        self.index = self.index + 4
        insert(buf, string.char(tonumber(hex, 16) % 128))
      else insert(buf, c) end
    else
      insert(buf, c)
    end
    self:read()
  end
  self:read()
  return { kind = "string", value = concat(buf) }
end
function Parser:readNumber()
  local start = self.index
  if self.cur == "-" then self:read() end
  local c = self.cur
  while c ~= nil and ((c >= "0" and c <= "9") or c == "." or c == "e" or c == "E" or c == "+" or c == "-") do
    self:read()
    c = self.cur
  end
  return { kind = "number", value = tonumber(sub(self.text, start, self.index - 1)) }
end
local function parse(text)
  local p = newParser(text)
  p:read()
  p:skipWhiteSpace()
  local v = p:readValue()
  p:skipWhiteSpace()
  if p.cur ~= nil then p:error("unexpected trailing characters") end
  return v
end

-- statistics and serializer
local nodes, numSum, strLen
local function walk(v)
  nodes = nodes + 1
  local k = v.kind
  if k == "number" then numSum = numSum + v.value
  elseif k == "string" then strLen = strLen + #v.value
  elseif k == "array" then for i = 1, #v.items do walk(v.items[i]) end
  elseif k == "object" then
    for i = 1, #v.names do strLen = strLen + #v.names[i]; walk(v.values[i]) end
  end
end
local function ser(v, out)
  if v == NULL then insert(out, "null") return end
  local k = v.kind
  if k == "number" then insert(out, string.format("%.14g", v.value))
  elseif k == "string" then insert(out, '"'); insert(out, v.value); insert(out, '"')
  elseif k == "bool" then insert(out, v.value and "true" or "false")
  elseif k == "array" then
    insert(out, "[")
    for i = 1, #v.items do if i > 1 then insert(out, ",") end ser(v.items[i], out) end
    insert(out, "]")
  else
    insert(out, "{")
    for i = 1, #v.names do
      if i > 1 then insert(out, ",") end
      insert(out, '"'); insert(out, v.names[i]); insert(out, '":')
      ser(v.values[i], out)
    end
    insert(out, "}")
  end
end

local text = buildDocument()
local rn, rs, rl, rser
for it = 1, N do
  local v = parse(text)
  nodes, numSum, strLen = 0, 0, 0
  walk(v)
  local out = {}
  ser(v, out)
  rn, rs, rl, rser = nodes, numSum, strLen, #concat(out)
end
print("json", #text, rn, string.format("%.6f", rs), rl, rser)
