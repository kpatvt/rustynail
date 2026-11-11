-- N5: as str_unused but the loop compares s == T with a string of the table (interning-dependent equality)
local m = arg and arg[1] or "mixed"
local S = {}
for k = 1, 8 do S[k] = ("x"):rep(m == "mixed" and 34 + k or m == "short" and 10 + k or 50 + k) end
local T = S[3]
local acc = 0
for i = 1, 5000000 do
  local s = S[i % 8 + 1]
  if s == T then acc = acc + 1 end
end
print(acc)
