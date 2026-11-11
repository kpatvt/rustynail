local m = {}
for i = 1, 5 do m[i] = {} for j = 1, 5 do m[i][j] = i * j end end
local s = 0
for i = 1, 5 do for j = 1, 5 do s = s + m[i][j] end end
print(s, m[3][4], m[5][5])
local graph = {a = {"b", "c"}, b = {"d"}, c = {"d"}, d = {}}
local visited, order = {}, {}
local function dfs(n) if visited[n] then return end visited[n] = true for _, m2 in ipairs(graph[n]) do dfs(m2) end order[#order + 1] = n end
dfs("a")
print(order[1], order[2], order[3], order[4])
local cyc = {}
cyc.self = cyc
print(cyc.self.self.self == cyc)
