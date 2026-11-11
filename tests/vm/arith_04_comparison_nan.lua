local nan = 0.0 / 0.0
print(nan == nan, nan ~= nan, nan < nan, nan <= nan, nan > nan, nan >= nan)
print(nan < 1, 1 < nan, nan == 1, 1 ~= nan)
local z, nz = 0.0, -0.0
print(z == nz, 1 / z, 1 / nz, tostring(nz), tostring(z), z < nz, nz < z)
print(3 == 3.0000000000000001, -0.0 == 0, 0 == -0.0)
local t = {}
t[nz] = 1 t[z] = 2
print(t[0], t[0.0], t[-0.0])
print(pcall(function() t[nan] = 1 end))
print(t[nan])
