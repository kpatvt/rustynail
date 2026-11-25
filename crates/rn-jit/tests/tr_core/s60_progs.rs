//! Section 59 programs (`reroot`: a loop root recorded on the minority direction of a branch is flushed once and re-recorded steered away from that
//! path, docs/JIT_TRACE.md section 60). Each has a loop whose first recorded iteration can take the rare direction of a branch inside an inlined
//! callee, so most entries of the root leave through one pre-roll exit into a side trace; the re-record drops recordings that follow the old path
//! (no penalty) until one takes the other direction or the drops run out. Observers behind the re-record: errors raised on the side path with
//! `debug.getlocal`, a count hook installed from inside, collections, a phase change of the branch's majority.

use super::progs::Prog;

pub const S60_PROGS: &[Prog] = &[
    Prog {
        name: "s60_queens",
        small: false,
        src: r#"
-- the awfy queens loop: `getRowColumn` short-circuits over three boolean arrays; which direction the root records at `freeRows[r + 1]` is a
-- matter of the iteration that made the loop hot
local Q = {}
Q.__index = Q
function Q:getRowColumn(r, c)
  return self.freeRows[r + 1] and self.freeMaxs[c + r + 1] and self.freeMins[c - r + 8]
end
function Q:setRowColumn(r, c, v)
  self.freeRows[r + 1] = v
  self.freeMaxs[c + r + 1] = v
  self.freeMins[c - r + 8] = v
end
function Q:placeQueen(c)
  for r = 0, 7 do
    if self:getRowColumn(r, c) then
      self.queenRows[r + 1] = c
      self:setRowColumn(r, c, false)
      if c == 7 then return true end
      if self:placeQueen(c + 1) then return true end
      self:setRowColumn(r, c, true)
    end
  end
  return false
end
local function queens()
  local q = setmetatable({ freeRows = {}, freeMaxs = {}, freeMins = {}, queenRows = {} }, Q)
  for i = 1, 8 do q.freeRows[i] = true; q.queenRows[i] = -1 end
  for i = 1, 16 do q.freeMaxs[i] = true; q.freeMins[i] = true end
  local ok = q:placeQueen(0)
  local sig = 0
  for i = 1, 8 do sig = sig * 10 + q.queenRows[i] end
  return ok, sig
end
local good, sig = 0, 0
for it = 1, 60 do
  local ok, s = queens()
  if ok then good = good + 1 end
  sig = s
end
print(good, sig)
"#,
    },
    Prog {
        name: "s60_side_errors",
        small: false,
        src: r#"
-- the first hot iteration takes the rare branch (k % 5 == 0 is true at the recording); later the common branch raises errors inside the inlined
-- callee every so often (caught by pcall, the frame's locals read by debug.getlocal from a message handler), a count hook is installed from inside
-- the loop for one round, and collections run between rounds
local hits, seen = 0, {}
local function pick(t, k)
  if t[k] then
    return t[k] * 2
  end
  if k % 97 == 13 and t.boom then
    local secret = k * 3
    error({ k = k, secret = secret })
  end
  return -k
end
local function round(n, boom, off)
  local t = { boom = boom }
  for i = 1, n, 5 do t[i + off] = i end
  local acc = 0
  for k = 1, n do
    local v = pick(t, k)
    if v > 0 then acc = acc + v else acc = acc - 1 end
  end
  return acc
end
local function handler(e)
  local name, val = debug.getlocal(2, 3)
  return { e = e, name = name, val = val }
end
for r = 1, 12 do
  if r == 7 then
    debug.sethook(function() hits = hits + 1 end, "", 1000)
  elseif r == 8 then
    debug.sethook()
  end
  local ok, res = xpcall(round, handler, 400 + r * 31, r % 3 == 0, r % 5)
  if ok then
    seen[#seen + 1] = res
  else
    seen[#seen + 1] = string.format("err k=%s secret=%s %s=%s", tostring(res.e.k), tostring(res.e.secret), tostring(res.name), tostring(res.val))
  end
  collectgarbage()
end
print(table.concat(seen, " "), hits > 0)
"#,
    },
    Prog {
        name: "s60_phases",
        small: false,
        src: r#"
-- the majority direction of the branch changes between phases: the re-record may complete the avoided path again and again (drops, then the
-- constraint is given up); a type change of the loaded value in the last phase makes the type guard of the old path fail too
local function cls(x, lim)
  if x < lim then return x + 1 end
  if x % 2 == 0 then return x // 2 end
  return 3 * x + 1
end
local function run(n, lim, flt)
  local s = 0
  local v = {}
  for i = 1, n do v[i] = flt and (i * 1.5) or i end
  for i = 1, n do
    local y = cls(v[i], lim)
    s = s + y
  end
  return s
end
local out = {}
for p = 1, 9 do
  local lim = (p % 3 == 0) and 10 or ((p % 3 == 1) and 900 or 450)
  out[#out + 1] = tostring(run(1000, lim, p == 9))
end
print(table.concat(out, ","))
"#,
    },
];
