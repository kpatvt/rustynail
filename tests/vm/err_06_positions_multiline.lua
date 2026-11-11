local function f()
  local a = nil
  local b = 1 +
    a
  return b
end
print(pcall(f))
local function g()
  local t = {}
  return t.a
    .b
end
print(pcall(g))
local function h()
  local x = {
    1,
    2,
    nil + 1,
  }
end
print(pcall(h))
print(pcall(function()
  local s = "a" ..
    "b" ..
    {}
end))
print(pcall(function()
  return 1
    <
    {}
end))
print(pcall(function() 
  error(
    "multi"
  )
end))
