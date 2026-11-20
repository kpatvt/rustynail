-- H7: an open upvalue that aliases register 0 of the running (loop) frame: UVREL < 64, the alias guard must keep the loop out of the no-alias path
local function run()
  local x = 0
  local function inc() x = x + 1 end
  local s = 0
  for i = 1, 3000 do
    inc()
    x = x + 10
    s = s + x
  end
  return s, x
end
print(run())
print(run())
-- the same with the upvalue at registers 1 and 3 of the frame (a range test, not an exact slot)
local function run1()
  local pad = 5
  local x = 0
  local function inc() x = x + 1 end
  local s = 0
  for i = 1, 3000 do
    inc()
    x = x + 10
    s = s + x + pad
  end
  return s, x
end
print(run1())
print(run1())
local function run3()
  local a, b, c = 1, 2, 3
  local x = 0
  local function inc() x = x + 1 end
  local s = 0
  for i = 1, 3000 do
    inc()
    x = x + 10
    s = s + x + a + b + c
  end
  return s, x
end
print(run3())
print(run3())
