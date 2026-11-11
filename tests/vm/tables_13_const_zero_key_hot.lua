-- GETI / SETI with the constant key 0 (never in the array part): tier 1 calls the helper instead of leaving the compiled code.
-- Hot loops so the code is compiled; plain tables, metatables (__index / __newindex), non-tables, barriers (collectable values
-- stored into old tables), nil values.
local out = {}
local function emit(x) out[#out + 1] = tostring(x) end

local cur = {}
local s = 0
for i = 1, 3000 do cur[0] = i s = s + cur[0] end
emit(s) emit(cur[0])

-- levenshtein-like row update
local prev, row = {}, {}
for j = 0, 20 do prev[j] = j end
for i = 1, 300 do
  row[0] = i
  for j = 1, 20 do
    local a, b = prev[j] + 1, row[j - 1] + 1
    row[j] = a < b and a or b
  end
  prev, row = row, prev
end
emit(prev[0]) emit(prev[20]) emit(row[0])

-- __newindex / __index on a key-0 access
local log = {}
local p = setmetatable({}, {
  __newindex = function(t, k, v) log[#log + 1] = k rawset(t, k, v) end,
  __index = function(t, k) return -k - 1 end,
})
local q = 0
for i = 1, 500 do
  local v = p[0]
  q = q + v
  if i == 250 then p[0] = 99 end
end
emit(q) emit(#log) emit(p[0])
local fwd = setmetatable({}, {__index = {[0] = "zero"}, __newindex = log})
for i = 1, 300 do local _ = fwd[0] fwd[0] = i end
emit(fwd[0]) emit(rawget(fwd, 0)) emit(log[0])

-- nil values: setting and clearing key 0
local n = {}
local c = 0
for i = 1, 1000 do
  n[0] = (i % 2 == 0) and i or nil
  if n[0] then c = c + 1 end
end
emit(c)

-- non-table operands
local function get0(x) return x[0] end
local function set0(x) x[0] = 1 end
for i = 1, 300 do get0({}) set0({}) end
for _, v in ipairs{5, true, "abc", print} do
  local ok, e = pcall(get0, v)
  emit(tostring(ok) .. ":" .. (ok and tostring(e) or (e:gsub("^.-:%d+: ", ""))))
  local ok2, e2 = pcall(set0, v)
  emit(tostring(ok2) .. ":" .. (ok2 and "" or (e2:gsub("^.-:%d+: ", ""))))
end

-- barrier: collectable values stored under key 0 of an old table across collections
local old = {}
collectgarbage() collectgarbage()
local alive = 0
for i = 1, 400 do
  old[0] = {i}
  if i % 50 == 0 then collectgarbage() end
  if i % 7 == 0 then collectgarbage("step") end
  alive = alive + old[0][1]
end
emit(alive)
-- integer 0 vs float 0.0 vs string "0"
local f = {}
for i = 1, 300 do f[0.0] = i f["0"] = -i end
emit(f[0]) emit(f["0"]) emit(next({[0] = 1}))

for i = 1, #out do print(out[i]) end
