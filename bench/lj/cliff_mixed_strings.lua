-- informational repro (task P-LJ): our trace is flushed "Ineffective" when a table holds short (<=40 B) and long (>40 B) strings and the loop reads them (30x slower than all-short or all-long). Run: rnlua bench/lj/cliff_mixed_strings.lua (cycles: 28M with rep(10+k) or rep(50+k), 890M with rep(34+k))
-- LICM-defeating variant of bench/lib/sel_count.lua (task P-LJ): the argument varies with the iteration, so LuaJIT cannot fold/hoist the call
local S = {}
for k = 1, 8 do S[k] = ("x"):rep(34 + k) end
local fmt, sub, find, rep, byte, char = string.format, string.sub, string.find, string.rep, string.byte, string.char
local function cnt(...) return select("#", ...) end
local acc = 0
for i = 1, 5360592 do
local s = S[i % 8 + 1]
acc = acc + cnt(1, 2, 3, i % 2)
end
print(acc)
