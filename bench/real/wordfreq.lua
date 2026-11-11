-- text processing: generate pseudo-random text, word frequency, top-N, line statistics
local words = {}
for w in ("the quick brown fox jumps over lazy dog and runs away from big bad wolf while singing a song about love war peace time money data system program function value table string number thread yield resume error"):gmatch("%a+") do words[#words + 1] = w end
local seed = 42
local function rnd(n) seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed % n + 1 end
local lines = {}
for i = 1, 2000 do
  local t = {}
  for j = 1, 5 + rnd(10) do
    local w = words[rnd(#words)]
    if rnd(10) == 1 then w = w:upper() elseif rnd(10) == 1 then w = w:sub(1, 1):upper() .. w:sub(2) end
    t[#t + 1] = w .. (rnd(8) == 1 and "," or "")
  end
  lines[i] = table.concat(t, " ") .. "."
end
local text = table.concat(lines, "\n")
local result
for iter = 1, 12 do
  local freq, total = {}, 0
  for w0 in text:gmatch("%a+") do
    local w = w0:lower()
    freq[w] = (freq[w] or 0) + 1
    total = total + 1
  end
  local arr = {}
  for w, c in pairs(freq) do arr[#arr + 1] = { w = w, c = c } end
  table.sort(arr, function(a, b) if a.c ~= b.c then return a.c > b.c end return a.w < b.w end)
  local longest = 0
  for line in text:gmatch("[^\n]+") do
    local n = select(2, line:gsub("%S+", ""))
    if n > longest then longest = n end
  end
  result = string.format("%d %s=%d %s=%d longest=%d", total, arr[1].w, arr[1].c, arr[2].w, arr[2].c, longest)
end
print(result)
