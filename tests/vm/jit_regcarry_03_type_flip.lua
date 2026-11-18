-- a slot the outer iteration carries changes type between outer iterations: the link's types no longer prove the root's entry
local function run(n, m, flip)
  local acc, k = 0, 1
  local out = {}
  for i = 1, n do
    local base = i
    local f = flip(i)
    for j = 1, m do
      acc = acc + base * j
      k = k + 1
    end
    acc = acc + f
    out[#out + 1] = math.type(acc) or type(acc)
  end
  return acc, k, table.concat(out, ",")
end
local function flip_none(i) return 1 end
local function flip_f(i) return (i % 3 == 0) and 0.5 or 1 end
local function flip_s(i) return (i % 5 == 0) and "2" or 3 end
local r = {}
for k = 1, 60 do run(8, 5, flip_none) end
for k = 1, 3 do r[#r + 1] = table.concat({ run(8, 5, flip_none) }, ";") end
for k = 1, 60 do run(8, 5, flip_f) end
for k = 1, 3 do r[#r + 1] = table.concat({ run(8, 5, flip_f) }, ";") end
for k = 1, 30 do run(8, 5, flip_s) end
for k = 1, 3 do r[#r + 1] = table.concat({ run(8, 5, flip_s) }, ";") end
for k = 1, 30 do run(8, 5, flip_none) end
r[#r + 1] = table.concat({ run(8, 5, flip_none) }, ";")
print(table.concat(r, "|"))
