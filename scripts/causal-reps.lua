-- Causal study (docs/STUDY_CAUSAL_GAP.md, deliverable 3): run one benchmark main chunk N times in ONE process.
--   <lua> scripts/causal-reps.lua bench/<p>.lua            (N from the environment variable CAUSAL_REPS, default 1)
-- The chunk is loaded once (one set of prototypes: traces and compiled code survive between repetitions); each repetition
-- calls it again with an empty `arg` (default size). cycles(N) - cycles(N-1) is the cost of the N-th repetition; the first
-- repetition minus a later one is the warm-up (recording, compiling, formation, cold caches) of the engine.
-- Works on Lua 5.1 (LuaJIT) and 5.4 / 5.5.
local path = assert(arg and arg[1], "usage: causal-reps.lua <file>")
local n = tonumber(os.getenv("CAUSAL_REPS") or "1")
local f = assert(loadfile(path))
for _ = 1, n do
  arg = { [0] = path }
  f()
end
