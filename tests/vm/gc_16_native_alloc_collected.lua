-- library functions that allocate (table.pack, string.gmatch, load, debug.getinfo, ...) must
-- trigger collection: memory stays bounded across many calls
local fs = {
  function() return table.pack(1, 2) end,
  function() return string.gmatch("abc", "b") end,
  function() return load("return 1") end,
  function() return debug.getinfo(1) end,
  function() return io.tmpfile() end,
}
collectgarbage()
local worst = 0
for _, f in ipairs(fs) do
  for i = 1, 100000 do
    f()
    if i % 1000 == 0 then
      local c = collectgarbage("count")
      if c > worst then worst = c end
    end
  end
end
print(worst < 60000)
