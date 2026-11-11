-- `next` with short-string keys (`findindex` fast path), including keys whose entries were
-- cleared and collected during the traversal (dead keys), and native calls adjusting their
-- results inline to 0, 2 or 3 wanted values (`pre_call_native_fast`).
local t = {}
for i = 1, 200 do t["k" .. i] = i end
for i = 1, 50 do t[i] = -i end
t[1.5] = "f"; t[true] = "b"
local seen, sum = 0, 0
for k, v in pairs(t) do
  seen = seen + 1
  if type(v) == "number" then sum = sum + v end
  if type(k) == "string" and v % 3 == 0 then
    t[k] = nil -- clearing a field during traversal is allowed
    collectgarbage() -- the cleared entry may become a dead key
  end
end
print(seen, sum)
local left = 0
for k in pairs(t) do left = left + 1 end
print(left)
-- explicit next() chains with string keys of many lengths (short and long strings)
local u = {}
for i = 1, 60 do u[string.rep("s", i)] = i end
local n, k = 0, nil
repeat
  k = next(u, k)
  if k then n = n + u[k] end
until k == nil
print(n)
print(pcall(next, u, "absent-key"), pcall(next, {}, "x"))

-- native results adjusted inline
local function three() local a, b, c = string.byte("xy", 1, -1); return a, b, c end
local function three5() local a, b, c = select(1, 1, 2, 3, 4, 5); return a, b, c end
local function two0() local a, b = string.byte(""); return a, b end
local function zero() string.byte("abc", 1, 3) end
print(three()); print(three5()); print(two0()); print(zero())
local co = coroutine.create(function(...) coroutine.yield(...) return "done", 1, 2, 3 end)
local ok, a, b = coroutine.resume(co, 10, 20, 30)
print(ok, a, b)
local ok2, x, y, z = coroutine.resume(co)
print(ok2, x, y, z)
for i = 1, 3 do local p, q, r = math.max(i, 2), string.find("hello", "l+") ; print(p, q, r) end
