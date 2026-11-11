local function rec() return rec() + 1 end
print(xpcall(rec, function(m) return "H:" .. m end))
print(xpcall(rec, function(m) local x = nil return x.y end))
print(xpcall(function() error("a", 0) end, function(m) error("b", 0) end))
print(select('#', xpcall(rec, print)))
