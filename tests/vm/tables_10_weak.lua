local wk = setmetatable({}, {__mode = "k"})
local wv = setmetatable({}, {__mode = "v"})
local keep = {}
wk[keep] = 1 wk[{}] = 2
wv[1] = keep wv[2] = {} wv[3] = "str" wv[4] = 42
collectgarbage()
local nk = 0 for _ in pairs(wk) do nk = nk + 1 end
print(nk, wk[keep], wv[1] == keep, wv[2], wv[3], wv[4])
local both = setmetatable({}, {__mode = "kv"})
both[1] = {} both[keep] = keep both[2] = 5
collectgarbage()
print(both[1], both[keep] == keep, both[2])
