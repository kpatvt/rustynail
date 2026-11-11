
local f, a, b = ...
f()
f(a)
f(a, b)
f(a, f(b))
f(f(f()))
f{1, 2}
f"s"
f[[s]]
f(...)
f(a, ...)
f((...))
return f(a), (f(b)), f(...), ...
