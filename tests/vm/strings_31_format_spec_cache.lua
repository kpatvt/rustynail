-- BUNDLE-B S2: string.format with repeated (cached) format strings: every conversion, widths/precision/flags,
-- error order and messages, __tostring/__name per call, address reuse and GC stress.
local function strip(e) return (tostring(e):gsub("^[^:]*:%d+: ", "")) end
local function try(f, ...)
  local r = table.pack(pcall(f, ...))
  if r[1] then return (tostring(r[2]):gsub("0x%x+", "PTR")) end
  return "ERR " .. strip(r[2])
end

local specs = {
  "%d", "%i", "%5d", "%-5d|", "%05d", "%+d", "% d", "%.3d", "%5.3d", "%x", "%X", "%08x", "%5x", "%-8X|", "%#x", "%#o",
  "%o", "%u", "%c", "%5c", "%-5c|", "%s", "%10s", "%-10s|", "%.2s", "%10.2s", "%.s", "%f", "%.2f", "%10.3f", "%-10.1f|",
  "%+.0f", "%e", "%E", "%g", "%G", "%.14g", "%a", "%A", "%q", "%p", "%%", "%5%", "%y", "%", "%5", "%ld", "%.123d", "%123d",
  "%0-5d", "%+5s", "%#5s", "%.30f", "%--5d", "%1234567890123456789012345678901234567890d", "%.f", "%5.1s", "%#.3x", "%099d",
  "%.99f", "%-5.2s|", "%i5", "%5i", "%.0s", "%3c",
}
local args = { 65, 255, -7, 3.0, 2.5, 1e100, -0.0, "str", "12", "a\0b", "hello world", true, nil, {}, 1 << 62, math.mininteger,
  1 / 0, -1 / 0, 0 / 0 }
for round = 1, 3 do -- round 1: first sight, 2: compile, 3: cached
  local out = {}
  for _, sp in ipairs(specs) do
    local fmt = "<" .. sp .. ">"
    for i = 1, 19 do
      out[#out + 1] = sp .. " " .. i .. " " .. try(string.format, fmt, args[i])
    end
    out[#out + 1] = sp .. " none " .. try(string.format, fmt)
    out[#out + 1] = sp .. " two " .. try(string.format, fmt .. fmt, 65, "s")
  end
  local h = 0
  for _, l in ipairs(out) do h = (h * 31 + #l) % 1000003 end
  print("round", round, #out, h)
  if round == 1 then for _, l in ipairs(out) do print(l) end end
  if round > 1 then
    -- identical to round 1 by construction of the hash line; spot check text too
    print(out[1], out[#out])
  end
end

-- error order: the arguments are checked when each item is reached, earlier items run first
print(try(string.format, "%d %d", 1))
print(try(string.format, "%d %y", 1, 2))
print(try(string.format, "%d %y", 1))
print(try(string.format, "%s %d", setmetatable({}, { __tostring = function() error("boom") end }), 1))
print(try(string.format, "%5.2f %d", "x", 1))
print(try(string.format, "%d", 1.5))
print(try(string.format, "%x", 2^63))
print(try(string.format, "%1234d", 1))
print(try(string.format, "%10.123q", 1))
print(try(string.format, "%s", "a\0b"))
print(try(string.format, "%5s", "a\0b"))
print(try(string.format, "%q", {}))

-- __tostring / __name are consulted on every call
local n = 0
local o = setmetatable({}, { __tostring = function() n = n + 1; return "obj" .. n end })
for i = 1, 4 do print(string.format("[%s][%-6s][%6s]", o, o, o)) end
local named = setmetatable({}, { __name = "My.Type" })
for i = 1, 3 do print((string.format("%s", named):gsub("0x%x+", "PTR"))) end
local t = setmetatable({}, { __tostring = function() return 42 end })
for i = 1, 3 do print(try(string.format, "%s", t)) end

-- dynamic format strings: different bytes over time (address reuse after collection)
collectgarbage("stop")
local acc = {}
for i = 1, 400 do
  local f = "v" .. (i % 7) .. "=%" .. (i % 3 == 0 and "d" or i % 3 == 1 and "5.1f" or "x") .. "|"
  acc[#acc + 1] = string.format(f, i) .. string.format(f, i * 2)
  if i % 50 == 0 then collectgarbage("restart"); collectgarbage(); collectgarbage("stop") end
end
collectgarbage("restart")
print(#acc, acc[1], acc[2], acc[7], acc[400])
local h = 0
for _, s in ipairs(acc) do h = (h * 33 + #s + s:byte(-2)) % 1000003 end
print(h)

-- many distinct formats cycling through the cache
local seen = {}
for i = 1, 200 do
  local f = string.rep("%d,", i % 9 + 1) .. i
  local a = {}
  for k = 1, i % 9 + 1 do a[k] = k * i end
  local r = string.format(f, table.unpack(a))
  seen[#seen + 1] = r
  r = string.format(f, table.unpack(a))
  assert(r == seen[#seen])
end
print(#seen, seen[1], seen[200])
print(collectgarbage("count") > 0)
