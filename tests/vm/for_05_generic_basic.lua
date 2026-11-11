local t = {10, 20, 30, nil, 50}
for i, v in ipairs(t) do io.write(i, "=", v, " ") end print()
local keys = {}
for k, v in pairs({a = 1, b = 2, c = 3}) do keys[#keys + 1] = k .. v end
local s = {}
for _, v in ipairs(keys) do s[v] = true end
print(s.a1, s.b2, s.c3, #keys)
for k, v in next, {5, 6, 7} do io.write(k, ":", v, " ") end print()
local function iter(n) local i = 0 return function() i = i + 1 if i <= n then return i, i * i end end end
for i, sq in iter(4) do io.write(i, "^2=", sq, " ") end print()
local function stateless(s, c) if c < s then return c + 1, "v" .. c + 1 end end
for i, v in stateless, 3, 0 do io.write(i, v, " ") end print()
for a, b, c, d in function(s, c) if c == nil then return 1, 2, 3, 4 end end do print(a, b, c, d) end
