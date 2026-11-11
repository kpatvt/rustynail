-- N5: tables vs functions in a table slot, read and unused. arg: mixed | tab
local m = arg and arg[1] or "mixed"
local T = {}
for k = 1, 8 do T[k] = (m == "mixed" and k % 2 == 0) and print or {k} end
local acc = 0
for i = 1, 5000000 do
  local o = T[i % 8 + 1]
  acc = acc + i % 3
end
print(acc)
