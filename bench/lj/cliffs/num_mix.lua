-- N5: int vs float values read from a table, feeding arithmetic. arg: mixed | int | float
local m = arg and arg[1] or "mixed"
local T = {}
for k = 1, 8 do T[k] = (m == "int" or (m == "mixed" and k % 2 == 0)) and k or k + 0.5 end
local acc = 0
for i = 1, 5000000 do
  acc = acc + T[i % 8 + 1]
end
print(acc)
