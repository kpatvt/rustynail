-- N5 cliff repro: loop reads strings of mixed length class (short <= 40 B, long > 40 B) and does not use them.
-- arg: mixed | short | long
local m = arg and arg[1] or "mixed"
local S = {}
for k = 1, 8 do S[k] = ("x"):rep(m == "mixed" and 34 + k or m == "short" and 10 + k or 50 + k) end
local acc = 0
for i = 1, 5000000 do
  local s = S[i % 8 + 1]
  acc = acc + i % 3
end
print(acc)
