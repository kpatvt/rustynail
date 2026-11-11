local function encode(v, out)
  local t = type(v)
  if t == "table" then
    if #v > 0 or next(v) == nil then
      out[#out + 1] = "["
      for i = 1, #v do if i > 1 then out[#out + 1] = "," end; encode(v[i], out) end
      out[#out + 1] = "]"
    else
      out[#out + 1] = "{"
      local keys = {}
      for k in pairs(v) do keys[#keys + 1] = k end
      table.sort(keys)
      for i, k in ipairs(keys) do if i > 1 then out[#out + 1] = "," end; out[#out + 1] = string.format("%q:", k); encode(v[k], out) end
      out[#out + 1] = "}"
    end
  elseif t == "string" then out[#out + 1] = '"' .. v:gsub('[%c"\\]', function(c) return string.format("\\u%04x", c:byte()) end) .. '"'
  else out[#out + 1] = tostring(v) end
end
local function decode(s)
  local pos = 1
  local function ws() pos = s:find("%S", pos) or #s + 1 end
  local value
  local function str() local e = pos + 1; local b = {}; while true do local c = s:sub(e, e); if c == '"' then break elseif c == "\\" then local h = s:sub(e + 2, e + 5); b[#b + 1] = string.char(tonumber(h, 16)); e = e + 6 else b[#b + 1] = c; e = e + 1 end end; pos = e + 1; return table.concat(b) end
  function value()
    ws(); local c = s:sub(pos, pos)
    if c == "{" then pos = pos + 1; local r = {}; ws(); if s:sub(pos, pos) == "}" then pos = pos + 1; return r end
      while true do ws(); local k = str(); ws(); pos = pos + 1; r[k] = value(); ws(); local d = s:sub(pos, pos); pos = pos + 1; if d == "}" then return r end end
    elseif c == "[" then pos = pos + 1; local r = {}; ws(); if s:sub(pos, pos) == "]" then pos = pos + 1; return r end
      while true do r[#r + 1] = value(); ws(); local d = s:sub(pos, pos); pos = pos + 1; if d == "]" then return r end end
    elseif c == '"' then return str()
    else local num = s:match("^-?%d+%.?%d*[eE]?[+-]?%d*", pos); if num and #num > 0 then pos = pos + #num; return tonumber(num) end
      local w = s:match("^%a+", pos); pos = pos + #w; if w == "true" then return true elseif w == "false" then return false end; return nil end
  end
  return value()
end
local n = tonumber(arg and arg[1]) or 300
local doc = {}
for i = 1, n do doc[i] = {id = i, name = "user" .. i, tags = {"a", "b", "c" .. i}, score = i * 1.5, active = i % 2 == 0, nested = {x = i, y = {i, i + 1}}} end
local total = 0
for r = 1, 150 do
  local out = {}
  encode(doc, out)
  local s = table.concat(out)
  local back = decode(s)
  total = total + #s + #back + back[n].nested.y[2]
end
print(total)
