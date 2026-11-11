-- require of a medium module (1500 lines of generated functions) and one call into it
local dir = arg[0]:match("^(.*/)") or "./"
package.path = dir .. "?.lua;" .. package.path
local m = require("mod_medium")
print(m.f1(2))
