-- Formation study (docs/STUDY_FORMATION.md): LuaJIT's trace formation events with CPU-time stamps, for one run of a benchmark.
--   luajit scripts/formation-lj.lua bench/<p>.lua        (CAUSAL_REPS=N repetitions of the chunk, default 1)
-- Prints to stderr, one line per event: `<seconds since start> <what> <trace> <parent trace or abort code> <exit or abort info>`
-- (`what`: start / stop / abort / flush), and at the end `END <seconds>`. Events are kept in a preallocated table and printed after the
-- run, so the callback costs one table store per field. Times are os.clock() (process CPU time; convert with cycles / task-clock).
local path = assert(arg and arg[1], "usage: formation-lj.lua <file>")
local n = tonumber(os.getenv("CAUSAL_REPS") or "1")
local clock = os.clock
local ev, k = {}, 0
local t0 = clock()
jit.attach(function(what, tr, func, pc, otr, oex)
  ev[k + 1], ev[k + 2], ev[k + 3], ev[k + 4], ev[k + 5] = clock() - t0, what, tr or -1, otr or -1, oex or -1
  k = k + 5
end, "trace")
local f = assert(loadfile(path))
for _ = 1, n do
  arg = { [0] = path }
  f()
end
local tend = clock() - t0
for i = 1, k, 5 do
  io.stderr:write(string.format("%.6f %s %s %s %s\n", ev[i], ev[i + 1], tostring(ev[i + 2]), tostring(ev[i + 3]), tostring(ev[i + 4])))
end
io.stderr:write(string.format("END %.6f\n", tend))
