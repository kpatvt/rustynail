local Account = {}; Account.__index = Account
function Account.new(b) return setmetatable({balance = b, count = 0}, Account) end
function Account:deposit(v) self.balance = self.balance + v; self.count = self.count + 1 end
function Account:withdraw(v) if v > self.balance then return false end; self.balance = self.balance - v; self.count = self.count + 1; return true end
function Account:get() return self.balance end
local Savings = setmetatable({}, {__index = Account}); Savings.__index = Savings
function Savings.new(b) local o = Account.new(b); o.rate = 2; return setmetatable(o, Savings) end
function Savings:interest() self:deposit(math.floor(self.balance * self.rate / 100) % 1000) end  -- bounded: the original overflows int64
local n = tonumber(arg and arg[1]) or 9000000
local a, s = Account.new(100), Savings.new(1000)
local ok = 0
for i = 1, n do
  a:deposit(i % 13); if a:withdraw(i % 7) then ok = ok + 1 end
  s:deposit(1); if i % 100 == 0 then s:interest() end
  ok = ok + (a:get() > 0 and 1 or 0)
end
print(string.format("%d", ok), string.format("%d", a:get()), string.format("%d", s:get()), a.count, s.count)
