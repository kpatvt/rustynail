-- Helpers shared by the tests/ext programs. Output must be identical under LuaJIT 2.1 and rnlua, so numbers are
-- printed with %.14g (no integer/float subtype difference) and error messages are normalised.
local C = {}

function C.hex(s)
  s = tostring(s)
  local t = {}
  for i = 1, #s do t[#t + 1] = string.format("%02x", s:byte(i)) end
  return table.concat(t, " ")
end

function C.num(v) if v ~= v then return "nan" end return string.format("%.14g", v) end

local function q(s)
  return '"' .. s:gsub("[^%w ]", function(c) return string.format("\\x%02x", c:byte()) end) .. '"'
end
C.q = q

local order = { number = 1, string = 2, boolean = 3, table = 4 }
local function keylt(a, b)
  local ta, tb = type(a), type(b)
  if ta ~= tb then return (order[ta] or 9) < (order[tb] or 9) end
  if ta == "number" or ta == "string" then return a < b end
  if ta == "boolean" then return (not a) and b end
  return false
end

-- canonical text of a value: sorted keys, strings escaped, numbers %.14g, tables nested
function C.show(v, depth)
  depth = depth or 0
  local tv = type(v)
  if tv == "nil" then return "nil"
  elseif tv == "boolean" then return tostring(v)
  elseif tv == "number" then return C.num(v)
  elseif tv == "string" then return q(v)
  elseif tv == "table" then
    if depth > 30 then return "{...}" end
    local keys = {}
    for k in pairs(v) do keys[#keys + 1] = k end
    table.sort(keys, keylt)
    local parts = {}
    for _, k in ipairs(keys) do
      parts[#parts + 1] = "[" .. C.show(k, depth + 1) .. "]=" .. C.show(v[k], depth + 1)
    end
    return "{" .. table.concat(parts, ",") .. "}"
  elseif tv == "cdata" then return C.num(tonumber(v)) -- LuaJIT int64 from a decoded 0x10/0x11 tag
  else return "<" .. tv .. ">" end
end

local function norm(e)
  e = tostring(e)
  e = e:gsub("^[^:\n]*:%d+: ", "")
  e = e:gsub("to '[^']*'", "to '?'")
  e = e:gsub("0x%x+", "0xADDR")
  return e
end
C.norm = norm

-- P(label, f, ...): print the (shown) results of pcall(f, ...) in one line
function C.P(label, f, ...)
  local r = { pcall(f, ...) }
  local n = select("#", pcall(f, ...))
  local out = { label }
  if r[1] then
    out[2] = "ok"
    for i = 2, n do out[#out + 1] = C.show(r[i]) end
  else
    out[2] = "ERR"
    out[3] = norm(r[2])
  end
  print(table.concat(out, " | "))
end

-- deterministic pseudo-random numbers (identical in both engines: pure double arithmetic)
function C.rng(seed)
  local s = seed
  return function(n)
    s = (s * 48271) % 2147483647 -- MINSTD: products stay below 2^47, exact as double and as integer
    return s % n + 1
  end
end

return C
