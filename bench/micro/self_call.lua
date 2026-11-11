local N = 20000000
local o = {v = 1}; function o:get() return self.v end; local a; for i = 1, N do a = o:get(); a = o:get(); a = o:get(); a = o:get() end
print(0)
