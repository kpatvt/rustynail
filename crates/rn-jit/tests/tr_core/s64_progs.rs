//! Section 64 programs (`wgcall`: the start gate `wgate` weighs call-like instructions of a loop body, docs/JIT_TRACE.md section 64). The knob
//! only moves the moment of the first recording, so the programs are loops whose bodies call (C functions, Lua callees, methods, generic
//! `for` iterators, concatenations) with observers that must see identical state wherever the first trace starts: errors with
//! `debug.getlocal` / tracebacks, a count hook installed mid-run, coroutine yields from inside the called function, collections.

use super::progs::Prog;

pub const S64_PROGS: &[Prog] = &[
    Prog {
        name: "s64_call_loops",
        small: false,
        src: r#"
-- loops with calls of every kind; the gate's weight decides which of them records first
local fmt, sub = string.format, string.sub
local P = {}
P.__index = P
function P.new(v) return setmetatable({ v = v }, P) end
function P:get(k) return self.v + k end
local function f(a, b) return a * 2 + b end
local ring, acc, s = {}, 0, ""
local S = {}
for k = 1, 64 do S[k] = "item " .. k end
for o = 1, 40 do
  for k = 1, 64 do
    local r = fmt("%s", S[k])
    ring[k] = r
    acc = acc + #r + f(k, o)
  end
end
local p = P.new(3)
for i = 1, 3000 do acc = acc + p:get(i % 7) end
local t = {}
for i = 1, 200 do t[i] = i end
for r = 1, 30 do
  for _, v in ipairs(t) do acc = acc + v end
end
for i = 1, 500 do s = sub(s .. i, -20) end
print(acc, s, ring[64])
"#,
    },
    Prog {
        name: "s64_call_observers",
        small: false,
        src: r#"
-- the same loops with observers: an error with debug.getlocal and a traceback from inside the callee, a count hook installed mid-run,
-- coroutine yields from the called function, a full collection between rounds
local fmt = string.format
local function g(k, n)
  if k == n then
    local name, val = debug.getlocal(2, 1)
    error({ k = k, name = name, val = val, tb = (debug.traceback("x", 1):gsub("\n.*", "")) })
  end
  return k + 1
end
local acc = 0
for round = 1, 6 do
  local ok, e = pcall(function()
    local x = round
    for k = 1, 4000 do acc = acc + g(k, 2000 * round) + #fmt("%d", x) end
  end)
  if not ok then acc = acc + e.k; print(round, e.name, e.val, e.tb) end
  collectgarbage()
end
local hits = 0
for i = 1, 3000 do
  if i == 1500 then debug.sethook(function() hits = hits + 1 end, "", 97) end
  acc = acc + g(i, -1)
end
debug.sethook()
local co = coroutine.wrap(function()
  local function y(i) if i % 50 == 0 then coroutine.yield(i) end return i end
  local s = 0
  for i = 1, 5000 do s = s + y(i) end
  return -s
end)
local last
for _ = 1, 101 do last = co() end
print(acc, hits > 0, last)
"#,
    },
];
