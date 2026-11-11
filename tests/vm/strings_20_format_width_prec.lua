-- string.format %s / %c with flags, width and precision (native padding path), %f/%e/%g exactness
local function show(...)
  local ok, r = pcall(string.format, ...)
  if ok then print(("[%s]"):format(r:gsub("%z", "\\0"):gsub("[\1-\31\127-\255]", function(c) return ("\\%d"):format(c:byte()) end))) else print("ERR", r) end
end
local strs = {"", "a", "abc", "hello world", ("x"):rep(99), ("y"):rep(100), ("z"):rep(150), "a\0b", "\255\1\2", "%d"}
local forms = {"%s", "%5s", "%-5s", "%--5s", "%.0s", "%.s", "%.3s", "%5.3s", "%-5.3s", "%10.2s", "%-10.2s", "%.99s", "%99s", "%-99s",
  "%99.100s", "%.100s", "%1s", "%0s", "%05s", "% s", "%+s", "%#s", "%5.s", "%-.s", "%.1s%.1s", "%3s|%-3s|", "%100s", "%.-1s", "%5.-1s",
  "%123s", "%.123s", "%5%", "%-s", "%-0s", "%0-5s"}
for _, f in ipairs(forms) do
  for _, s in ipairs(strs) do show(f, s) end
end
local cf = {"%c", "%5c", "%-5c", "%--5c", "%.1c", "%05c", "%+c", "%1c", "%99c", "%-99c", "%100c", "% c", "%c%c|%3c|%-3c|"}
for _, f in ipairs(cf) do
  for _, v in ipairs({0, 65, 255, 256, 321, -1, 3.0, "66", "x"}) do show(f, v, 66, 67, 68) end
end
show("%c", 1.5)
show("%s", setmetatable({}, {__tostring = function() return "T\0Z" end}))
show("%5s", setmetatable({}, {__tostring = function() return "tt" end}))
show("%-5s|", true)
show("%5s", nil)
show("%.2s", 12345)
show("%8s", 1.5)
show("%s %5s", "a")
show("%5s")
-- floats: %f / %e / %g / %.Nf over assorted values and precisions
local vals = {0, -0.0, 1, -1, 0.5, 1.5, 2.5, 0.1, 0.2, 0.3, 1/3, 2/3, 1/7, 100, 1e15, 1e16, 1e22, 1e23, 1.5e300, 5e-324, 2.2250738585072014e-308,
  123456789.123456789, 0.000123456, 9.9999999999999e14, 0.99999999999999995, 999999.5, 99999.95, 0.05, 0.005, 0.0005, 1.005, 2.675, 1e-5, 1e-7, 123456.789e-10,
  math.pi, -math.pi, math.huge, -math.huge, 0/0, 2^53, 2^53 + 2, 2^63, 2^64, 1e19, 1e20, 4.35, 4.45, 8.325, 0.045}
local ffs = {"%f", "%.0f", "%.1f", "%.2f", "%5.2f", "%-10.3f|", "%010.3f", "%+.2f", "% .2f", "%.10f", "%.15f", "%.19f", "%.20f", "%.30f",
  "%e", "%.0e", "%.3e", "%.14e", "%g", "%.3g", "%.14g", "%.15g", "%.17g", "%.1g", "%#g", "%#.0f", "%10.4g", "%G", "%E", "%.0g", "%.19g", "%.20g"}
for _, f in ipairs(ffs) do
  local t = {}
  for _, v in ipairs(vals) do t[#t + 1] = (string.format(f, v)) end
  print(f, table.concat(t, " "))
end
-- tostring / concat of floats and ints
local t = {}
for _, v in ipairs(vals) do t[#t + 1] = tostring(v) .. "|" .. v end
print(table.concat(t, " "))
local seed = 12345
local function rnd() seed = (seed * 1103515245 + 12345) % 2147483648; return seed end
local out = {}
for i = 1, 3000 do
  local m = rnd() / 2147483648
  local e = rnd() % 60 - 30
  local v = m * 10.0 ^ e
  if i % 3 == 0 then v = math.floor(v * 1000) / 1000 end
  if i % 7 == 0 then v = -v end
  out[#out + 1] = tostring(v) .. " " .. string.format("%.14g %.3f %.2f %g %.5e", v, v, v, v, v)
end
for i = 1, #out, 97 do print(out[i]) end
local h = 0
for i = 1, #out do for c in out[i]:gmatch(".") do h = (h * 31 + c:byte()) % 1000000007 end end
print(h)
