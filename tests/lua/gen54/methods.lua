
local o = ...
o:a():b(1):c(1, 2):d "str" :e {1, 2} :f(...)
local x = o.a.b.c:d(o.e[1], o["f"]):g()
o.a.b:c(function(...) return ... end)
a.b.c.d = o:m()
return o:x(), o:y()
