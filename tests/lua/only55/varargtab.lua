local function f(...t)
  return t.n, ...
end
return f(1, 2, 3)
