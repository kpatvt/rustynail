-- Section 77.2 (`flink`): frameful side traces (they start inside an inlined callee) that link back to the root head of the loop
-- through the root's guard-free second entry. The loop's slots change type behind some of the links (a number becomes a string or a
-- table, a counter becomes a float), the callee's exits are taken in rotation, and a deeper callee needs more stack than the root.

local Lexer = {}
Lexer.__index = Lexer
function Lexer.new(s) return setmetatable({ s = s, pos = 1, n = 0 }, Lexer) end
function Lexer:next()
  local b = string.byte(self.s, self.pos)
  if not b then return nil end
  self.pos = self.pos + 1
  if b == 32 then return "sp" end
  if b >= 48 and b <= 57 then return "num", b - 48 end
  if b == 44 then return "comma", 0.5 end
  if b == 59 then return "semi", {} end
  return "ch", string.char(b)
end

local function deep(a, b, c, d, e, f, g, h)
  local x1, x2, x3, x4, x5, x6, x7, x8 = a, b, c, d, e, f, g, h
  return x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8
end

local function run(src)
  local lx = Lexer.new(src)
  local cnt, sum, last, acc = 0, 0, nil, 0
  while true do
    local k, v = lx:next()
    if not k then break end
    cnt = cnt + 1
    if k == "num" then
      sum = sum + v
      last = v
    elseif k == "ch" then
      last = v
    elseif k == "comma" then
      sum = sum + v
      last = sum
    elseif k == "semi" then
      last = v
      acc = acc + deep(1, 2, 3, 4, 5, 6, 7, cnt % 3)
    end
    if type(last) == "table" then last = #last end
  end
  return cnt, sum, tostring(last), acc
end

local src = string.rep("ab 12,3;x 9 ;; 7,,q ", 60)
for r = 1, 25 do
  local c, s, l, a = run(src)
  if r % 5 == 0 then print("flink", r, c, s, l, a) end
end
local src2 = string.rep("1;2;3;a,b,", 80)
for r = 1, 10 do
  local c, s, l, a = run(src2)
  if r % 5 == 0 then print("flink2", r, c, s, l, a) end
end
