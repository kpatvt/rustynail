-- informational (task F3b): CONCAT of strings and numbers in a hot loop (SNEW / CONCAT on traces); the results are only measured
-- (a result used as a table key is not traced: the key guard would fail on nearly every iteration)
local N = tonumber(arg and arg[1]) or 2000000
local function f(n)
  local s = 0
  for i = 1, n do
    local a = "key" .. (i % 64)
    local b = a .. ":" .. i
    s = s + #b
  end
  return s
end
print(f(N))
