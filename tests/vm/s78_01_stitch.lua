-- docs/JIT_TRACE.md section 78.1 (`stitch`): continuation traces behind calls of non-recursive functions, side recordings that stitch
-- at C calls, the end of a gmatch iteration in side traces, pops into the caller's continuation, frame links behind a TFORPREP,
-- string.format without '%', "" .. s and string.gmatch(s, p) on trace. Every path is checked against the interpreter's output.
local N = tonumber(arg and arg[1]) or 3000

-- a command interpreter in the shape of corpus2/dev/text_adventure: parse with gmatch, dispatch through a table of handlers
local G = { log = {}, n = 0, chars = 0, at = "hall" }
function G:say(fmt, ...)
  local line = string.format(fmt, ...)
  self.log[#self.log + 1] = line
  if #self.log > 3 then table.remove(self.log, 1) end
  self.n = self.n + 1
  self.chars = self.chars + #line
end

local words_seen = 0
function G:parse(line)
  local words = {}
  for w in line:lower():gmatch("%a+") do
    if w ~= "the" and w ~= "a" then words[#words + 1] = w end
    words_seen = words_seen + 1
  end
  return words[1], words[2]
end

local H = {}
function H.go(g, dir)
  if not dir then return g:say("Nowhere to go.") end
  g.at = dir
  g:say("You go %s.", dir)
end
function H.take(g, item)
  if not item then return g:say("Take what?") end
  g:say("Taken: %s.", item)
end
function H.look(g)
  g:say("You are in the %s.", g.at)
end
function H.wait(g)
  g:say("Time passes.")
end

function G:exec(line)
  local verb, noun = self:parse(line)
  if not verb or not H[verb] then return self:say("I don't understand '%s'.", line) end
  H[verb](self, noun)
end

local cmds = { "go north", "take the lamp", "look", "wait", "go", "take", "Dance wildly", "go south", "LOOK around",
  "take a very long word that is longer than forty characters abcdefghijklmnopqrstuvwxyzabcdefghij" }
local seed = 7
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end
local function gen()
  local r = rnd(#cmds)
  return (rnd(2) == 1 and "" or "") .. cmds[r]
end
for i = 1, N do
  G:exec(gen())
end
print(G.n, G.chars, words_seen, G.at)
for i = 1, #G.log do print(G.log[i]) end

-- string.format of a varying format without '%' (the register guard of the recorded string)
local fmts = { "alpha", "beta", "gamma", "delta" }
local acc = 0
for i = 1, 2000 do
  local f = fmts[(i % 4) + 1]
  if i > 1500 then f = "x%dy" end
  local s = (i > 1500) and string.format(f, i) or string.format(f)
  acc = acc + #s
end
print("fmt0", acc)
-- the same call site with a format that gains a '%' escape (the result is no longer the format itself)
local f2 = { "plain", "words", "only", "here" }
local out = {}
for i = 1, 2000 do
  if i == 1700 then f2[2] = "100%%" end
  out[(i % 4) + 1] = string.format(f2[(i % 4) + 1])
end
print("fmt0b", out[1], out[2], out[3], out[4])

-- "" .. s with a left operand that stops being empty
local pre = ""
local cat = 0
for i = 1, 2000 do
  if i == 1200 then pre = "P" end
  local s = pre .. fmts[(i % 4) + 1]
  cat = cat + #s + (s:sub(1, 1) == "P" and 1 or 0)
end
print("concat", cat)

-- gmatch iterators created on trace: subject / pattern order, long matches, captures, an empty subject
local subj = { "one two three", "", "xx yy", "a-b-c", string.rep("q", 50) .. " z" }
local tot, longs = 0, 0
for i = 1, 3000 do
  local s = subj[(i % #subj) + 1]
  local p = (i % 7 == 0) and "(%a)%a*" or "%a+"
  for w in string.gmatch(s, p) do
    tot = tot + #w
    if #w > 40 then longs = longs + 1 end
  end
end
print("gmatch", tot, longs)

-- a side recording of a loop root that stitches at a C call it cannot record (`table.concat`) after enough steps
local tc = 0
local parts = { "a", "b", "c" }
for i = 1, 3000 do
  local x = i * 3
  if i % 3 == 0 then
    local y = x + 1
    local z = y * 2
    local w = z - x
    local v = w + y
    local u = v * 2
    tc = tc + #table.concat(parts, ",") + u % 7
  else
    tc = tc + 1
  end
end
print("sidecf", tc)

-- the iterator advanced by a call outside the loop, an error inside a handler caught by pcall, a collection mid-run
local it = string.gmatch("k1 k2 k3 k4", "%w+")
print(it())
local errs = 0
for i = 1, 800 do
  local ok = pcall(function()
    G:exec(i % 97 == 0 and "go" or cmds[(i % 4) + 1])
    if i % 211 == 0 then error("boom") end
  end)
  if not ok then errs = errs + 1 end
  if i % 300 == 0 then collectgarbage() end
end
print("errs", errs, G.n)
for w in it do io.write(w, " ") end
print()
