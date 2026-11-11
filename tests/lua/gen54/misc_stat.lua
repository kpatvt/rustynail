
local a <const> = 5
do end
;;;
local function f(a, b, ...) return a end
function t_x() end
local t = {}
function t.a() end
function t.a.b.c() end
function t:m() return self end
function t.a.b:m(x) return self, x end
return
