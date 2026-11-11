-- Counts the bytecodes a program executes (count hook with count 1; run with RUSTYNAIL_JIT=off, hooks disable the JIT anyway).
-- usage: rnlua pprof-count.lua prog.lua   -> prints "BYTECODES n" on stderr
local f = assert(loadfile(arg[1]))
local n = 0
debug.sethook(function() n = n + 1 end, "", 1)
f()
debug.sethook()
io.stderr:write("BYTECODES ", n, "\n")
