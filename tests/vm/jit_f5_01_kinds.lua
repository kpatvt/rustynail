-- F5: HSTORE into an existing field: the table has no metatable / a metatable without __newindex /
-- __newindex as a function / as a table; the slot is non-nil or nil at run time.
local log = {}
local function mk(kind)
  local t = {a = 1, b = 2, c = 3}
  if kind == 1 then
    setmetatable(t, {})
  elseif kind == 2 then
    setmetatable(t, {__index = function() return 0 end})
  elseif kind == 3 then
    local sink = {}
    setmetatable(t, {__newindex = function(tt, k, v) log[#log + 1] = k .. "=" .. tostring(v); rawset(sink, k, v) end})
  elseif kind == 4 then
    local other = {}
    setmetatable(t, {__newindex = other})
    rawset(t, "other", other)
  end
  return t
end
local out = {}
for kind = 0, 4 do
  for round = 1, 3 do
    local t = mk(kind)
    for i = 1, 400 do
      t.a = i
      t.b = t.b + 1
      if i % 50 == 0 then t.c = nil end
      t.c = i * 2
    end
    local other = rawget(t, "other")
    out[#out + 1] = string.format("%d/%d a=%s b=%s c=%s other=%s", kind, round, tostring(rawget(t, "a")),
      tostring(rawget(t, "b")), tostring(rawget(t, "c")), tostring(other and rawget(other, "c")))
  end
end
print(table.concat(out, "\n"))
print(#log, log[1], log[#log])
