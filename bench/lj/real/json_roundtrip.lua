-- JSON encode + decode round trip over a generated document (strings, numbers, arrays, objects)
local function encode(v, out)
  local t = type(v)
  if t == "table" then
    if #v > 0 or next(v) == nil then
      out[#out+1] = "["
      for i = 1, #v do if i > 1 then out[#out+1] = "," end encode(v[i], out) end
      out[#out+1] = "]"
    else
      out[#out+1] = "{"
      local keys = {}
      for k in pairs(v) do keys[#keys+1] = k end
      table.sort(keys)
      for i, k in ipairs(keys) do
        if i > 1 then out[#out+1] = "," end
        out[#out+1] = string.format("%q:", k)
        encode(v[k], out)
      end
      out[#out+1] = "}"
    end
  elseif t == "string" then
    out[#out+1] = '"' .. v:gsub('[%c"\\]', function(c) return string.format("\\u%04x", c:byte()) end) .. '"'
  elseif t == "number" then
    out[#out+1] = string.format("%.14g", v)
  else
    out[#out+1] = tostring(v)
  end
end

local function decode(s)
  local pos = 1
  local val
  local function ws() pos = s:find("%S", pos) or #s + 1 end
  function val()
    ws()
    local c = s:sub(pos, pos)
    if c == "{" then
      local r = {}
      pos = pos + 1; ws()
      if s:sub(pos, pos) == "}" then pos = pos + 1 return r end
      while true do
        ws()
        local k = val(); ws()
        pos = pos + 1 -- ':'
        r[k] = val(); ws()
        local d = s:sub(pos, pos); pos = pos + 1
        if d == "}" then return r end
      end
    elseif c == "[" then
      local r, n = {}, 0
      pos = pos + 1; ws()
      if s:sub(pos, pos) == "]" then pos = pos + 1 return r end
      while true do
        n = n + 1; r[n] = val(); ws()
        local d = s:sub(pos, pos); pos = pos + 1
        if d == "]" then return r end
      end
    elseif c == '"' then
      local e = pos + 1
      while true do
        local q = s:find('["\\]', e)
        if s:sub(q, q) == '"' then
          local str = s:sub(pos + 1, q - 1); pos = q + 1
          return (str:gsub("\\u(%x%x%x%x)", function(h) return string.char(tonumber(h, 16)) end))
        end
        e = q + 2
      end
    elseif s:find("^true", pos) then pos = pos + 4 return true
    elseif s:find("^false", pos) then pos = pos + 5 return false
    elseif s:find("^null", pos) then pos = pos + 4 return nil
    else
      local num = s:match("^-?%d+%.?%d*[eE]?[+-]?%d*", pos)
      pos = pos + #num
      return tonumber(num)
    end
  end
  return val()
end

local function make(n)
  local doc = {}
  for i = 1, n do
    doc[i] = { id = i, name = "item" .. i, price = i * 1.25, tags = { "a", "b" .. i % 7, "line\nbreak" },
               active = i % 3 == 0, dims = { w = i % 17, h = i % 13, d = (i % 5) + 0.5 } }
  end
  return doc
end

local doc = make(300)
local total = 0
for iter = 1, 40 do
  local out = {}
  encode(doc, out)
  local s = table.concat(out)
  local back = decode(s)
  assert(#back == #doc and back[300].name == "item300")
  total = total + #s
end
print(total)
