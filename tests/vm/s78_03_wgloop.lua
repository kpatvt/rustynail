-- docs/JIT_TRACE.md section 78.3 (`wgloop`): the start gate weighs the one C call of a loop body (`fmt` moved from a local, `up` an
-- upvalue); a loop around a Lua callee keeps the plain weight. The knob only moves the first recording: errors, a hook and collections
-- observe the same state wherever it happens.
local fmt = string.format
local S, ring, acc = {}, {}, 0
for k = 1, 64 do S[k] = "item " .. k end
for o = 1, 60 do
  for k = 1, 64 do
    local r = fmt("%s", S[k])
    ring[k] = r
    acc = acc + #r
  end
  if o == 30 then collectgarbage() end
end
local up = string.rep
local function g() local n = 0 for i = 1, 3000 do n = n + #up("ab", i % 5) end return n end
acc = acc + g()
local ok, e = pcall(function() for i = 1, 3000 do if i == 2500 then error("stop " .. fmt("%d", i)) end acc = acc + #fmt("%d", i) end end)
print(acc, ring[64], ok, e)
local lines = 0
debug.sethook(function() lines = lines + 1 end, "", 1000)
for i = 1, 4000 do acc = acc + #fmt("%x", i) end
debug.sethook()
print(acc, lines > 0)
