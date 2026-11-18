-- an error thrown behind many carried transitions: the handler reads the locals of the dead frame (the stack must hold the carried values)
local function work(t, n, bad)
  local sum, cnt, scale = 0.0, 0, 1.5
  for i = 1, n do
    local row = t[i]
    local a, b = row.a, row.b
    for j = 1, 3 do
      sum = sum + a * j + b
      cnt = cnt + 1
    end
    scale = scale + 0.25
    if i == bad then local x = row.missing + 1 end
  end
  return sum, cnt, scale
end
local t = {}
for i = 1, 40 do t[i] = { a = i * 0.5, b = i % 7 } end
for k = 1, 80 do work(t, 40, -1) end
local out = {}
for _, bad in ipairs { 3, 17, 40, 1 } do
  local ok, e = xpcall(work, function(m)
    local r = {}
    for l = 1, 12 do
      local name, v = debug.getlocal(2, l)
      if name then r[#r + 1] = name .. "=" .. (type(v) == "table" and "tbl" or tostring(v)) end
    end
    return table.concat(r, ",") .. " | " .. tostring(m)
  end, t, 40, bad)
  out[#out + 1] = tostring(ok) .. " " .. tostring(e)
end
print(table.concat(out, "\n"))
