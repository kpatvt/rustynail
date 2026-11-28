-- docs/JIT_TRACE.md section 81.1: an allocation exit of a loop-free function root (a short leaf root that makes a table, `fnleaf`) must not
-- arm the sampling tick. The next interpreted back-edge belongs to whatever runs next (here a chunk `load`ed per round and called once), and
-- the armed tick compiled that prototype in tier 1, one chunk per round (template_etlua: 80 -> 625 compiles). Under GC stress every
-- allocation of the root leaves at its room check. Outputs against the interpreter.
local insert = table.insert
local P = {}
P.__index = P
function P:push(kind, start, text)
  insert(self.chunks, { kind, start, text })
end
local p = setmetatable({ chunks = {} }, P)
local total = 0
for r = 1, 2000 do
  local f = function() return r end -- a closure per iteration: the loop itself is not traced
  local g = assert(load("local s = 0 for i = 1, 3 do s = s + i end return s"))
  p:push("code", f(), "x") p:push("=", r, "y") p:push("code", r, "z") p:push("-", r, "w") p:push("code", r, "v")
  p:push("code", r, "x") p:push("=", r, "y") p:push("code", r, "z") p:push("-", r, "w") p:push("code", f(), "v")
  total = total + g() + #p.chunks
  if #p.chunks > 200 then p.chunks = {} end
end
print(total)
