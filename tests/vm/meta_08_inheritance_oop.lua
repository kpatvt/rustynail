local Animal = {} Animal.__index = Animal
function Animal.new(name) return setmetatable({name = name}, Animal) end
function Animal:speak() return self.name .. " makes a sound" end
function Animal:kind() return "animal" end
local Dog = setmetatable({}, {__index = Animal}) Dog.__index = Dog
function Dog.new(name) local d = Animal.new(name) return setmetatable(d, Dog) end
function Dog:speak() return self.name .. " barks" end
local a, d = Animal.new("generic"), Dog.new("rex")
print(a:speak(), d:speak(), d:kind(), a:kind())
print(getmetatable(d) == Dog, rawget(Dog, "kind"), Dog.kind == Animal.kind)
local Stack = {} Stack.__index = Stack
function Stack.new() return setmetatable({n = 0}, Stack) end
function Stack:push(v) self.n = self.n + 1 self[self.n] = v return self end
function Stack:pop() local v = self[self.n] self[self.n] = nil self.n = self.n - 1 return v end
local s = Stack.new():push(1):push(2):push(3)
print(s:pop(), s:pop(), s.n)
