-- prototype_objects: imitates prototype-based OOP in a bank/inventory/shop simulation: objects
-- cloned from prototypes, __index delegation 4 levels deep, per-object overrides, mixins by
-- copying functions. Plain functions with explicit self, no colon methods. small: N=1
local N = tonumber(arg and arg[1]) or 1800

local seed = 8675309
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- ------------------------------------------------------------ prototype machinery
local function clone(proto, props)
  local obj = props or {}
  return setmetatable(obj, { __index = proto })
end

local function mixin(target, source)
  for k, v in pairs(source) do
    if type(v) == "function" then target[k] = v end
  end
  return target
end

local function depth_of(obj)
  local d, mt = 0, getmetatable(obj)
  while mt do
    d = d + 1
    mt = getmetatable(mt.__index)
  end
  return d
end

-- ------------------------------------------------------------ mixins
local Auditable = {
  record = function(self, what, amount)
    local log = rawget(self, "log")
    if not log then log = {}; self.log = log end
    log[#log + 1] = what .. ":" .. amount
    if #log > 8 then table.remove(log, 1) end
    self.events = (rawget(self, "events") or 0) + 1
  end,
}

local Taxable = {
  tax_for = function(self, amount)
    return math.floor(amount * self.tax_rate / 100)
  end,
}

-- ------------------------------------------------------------ bank domain
local Base = { id = 0, kind = "base", events = 0 }
Base.describe = function(self) return self.kind .. "#" .. self.id end

local Account = clone(Base, { kind = "basic", balance = 0, fee = 100, rate = 0, tax_rate = 15 })
mixin(Account, Auditable)
mixin(Account, Taxable)

Account.deposit = function(self, amount)
  self.balance = self.balance + amount
  self.record(self, "dep", amount)
  return true
end

Account.withdraw = function(self, amount)
  local total = amount + self.fee
  if self.balance - total < -self.overdraft then
    self.record(self, "denied", amount)
    return false
  end
  self.balance = self.balance - total
  self.record(self, "wd", amount)
  return true
end
Account.overdraft = 0

Account.accrue = function(self)
  local interest = math.floor(self.balance * self.rate / 1200)
  if interest > 0 then
    local tax = self.tax_for(self, interest)
    self.balance = self.balance + interest - tax
    self.record(self, "int", interest - tax)
  end
end

local Savings = clone(Account, { kind = "savings", rate = 24, limit_per_day = 3, withdrawals = 0 })
Savings.withdraw = function(self, amount)
  if self.withdrawals >= self.limit_per_day then
    self.record(self, "limit", amount)
    return false
  end
  self.withdrawals = self.withdrawals + 1
  return Account.withdraw(self, amount)         -- delegate to parent explicitly
end
Savings.new_day = function(self) self.withdrawals = 0 end

local Premium = clone(Savings, { kind = "premium", rate = 40, fee = 0, tax_rate = 10 })
local Business = clone(Account, { kind = "business", overdraft = 50000, fee = 250, rate = 6 })
Business.new_day = function() end

local function new_account(proto, id, balance)
  local a = clone(proto, { id = id, balance = balance })
  if proto == Premium and id % 4 == 0 then a.fee = 50 end        -- per-object override
  if proto == Business and id % 5 == 0 then a.overdraft = 120000 end
  return a
end

-- ------------------------------------------------------------ shop domain
local Item = clone(Base, { kind = "item", price = 100, stock = 0, sold = 0, tax_rate = 8 })
mixin(Item, Auditable)
mixin(Item, Taxable)
Item.sell = function(self, qty)
  if self.stock < qty then
    self.record(self, "short", qty)
    return 0
  end
  self.stock = self.stock - qty
  self.sold = self.sold + qty
  self.record(self, "sale", qty)
  local net = self.price_for(self, qty)
  return net + self.tax_for(self, net)
end
Item.price_for = function(self, qty) return self.price * qty end

local Perishable = clone(Item, { kind = "perishable", price = 60, shelf_days = 5, tax_rate = 4 })
Perishable.price_for = function(self, qty)
  local base = Item.price_for(self, qty)
  if self.age >= self.shelf_days - 1 then return math.floor(base / 2) end
  return base
end
Perishable.age = 0
Perishable.new_day = function(self)
  self.age = self.age + 1
  if self.age > self.shelf_days then
    self.record(self, "spoil", self.stock)
    self.stock, self.age = 0, 0
  end
end

local Frozen = clone(Perishable, { kind = "frozen", price = 250, shelf_days = 20, tax_rate = 6 })
local Gadget = clone(Item, { kind = "gadget", price = 1999, tax_rate = 20 })
Gadget.new_day = function() end

-- ------------------------------------------------------------ build the world
local accounts, items = {}, {}
local ACCOUNT_PROTOS = { Account, Savings, Premium, Business }
for i = 1, N * 8 do
  accounts[i] = new_account(ACCOUNT_PROTOS[rnd(4)], i, rnd(100000))
end
local ITEM_PROTOS = { Item, Perishable, Frozen, Gadget }
for i = 1, N * 3 do
  local it = clone(ITEM_PROTOS[rnd(4)], { id = i, stock = rnd(40) + 10 })
  if i % 7 == 0 then it.price = it.price + 50 end                -- per-object override
  items[i] = it
end

local revenue, sales_count, denied = 0, 0, 0
local days = 30
for day = 1, days do
  for _ = 1, N * 10 do
    local a = accounts[rnd(#accounts)]
    local r = rnd(10)
    if r <= 4 then
      a.deposit(a, rnd(20000))
    elseif r <= 8 then
      if not a.withdraw(a, rnd(15000)) then denied = denied + 1 end
    else
      local b = accounts[rnd(#accounts)]
      local amount = rnd(10000)
      if b ~= a and a.withdraw(a, amount) then b.deposit(b, amount) else denied = denied + 1 end
    end
  end
  for _ = 1, N * 6 do
    local it = items[rnd(#items)]
    local got = it.sell(it, rnd(4))
    if got > 0 then revenue = revenue + got; sales_count = sales_count + 1 end
    if it.stock < 5 and rnd(3) == 1 then it.stock = it.stock + 30 end
  end
  for _, a in ipairs(accounts) do
    if a.new_day then a.new_day(a) end
  end
  for _, it in ipairs(items) do
    if it.new_day then it.new_day(it) end
  end
  if day % 10 == 0 then
    for _, a in ipairs(accounts) do a.accrue(a) end
  end
end

-- ------------------------------------------------------------ reports
local function sorted_keys(t)
  local ks = {}
  for k in pairs(t) do ks[#ks + 1] = k end
  table.sort(ks)
  return ks
end

print(string.format("accounts=%d items=%d days=%d denied=%d", #accounts, #items, days, denied))
local by_kind = {}
for _, a in ipairs(accounts) do
  local g = by_kind[a.kind]
  if not g then g = { count = 0, total = 0, overrides = 0, events = 0 }; by_kind[a.kind] = g end
  g.count = g.count + 1
  g.total = g.total + a.balance
  g.events = g.events + a.events
  if rawget(a, "fee") or rawget(a, "overdraft") then g.overrides = g.overrides + 1 end
end
for _, k in ipairs(sorted_keys(by_kind)) do
  local g = by_kind[k]
  print(string.format("  %-9s n=%4d total=%12d avg=%8.2f overrides=%d events=%d depth=%d", k, g.count,
    g.total, g.total / g.count, g.overrides, g.events, depth_of(({ basic = Account, savings = Savings,
      premium = Premium, business = Business })[k])))
end

local by_item = {}
for _, it in ipairs(items) do
  local g = by_item[it.kind]
  if not g then g = { count = 0, stock = 0, sold = 0, depth = depth_of(it) }; by_item[it.kind] = g end
  g.count = g.count + 1
  g.stock = g.stock + it.stock
  g.sold = g.sold + it.sold
end
for _, k in ipairs(sorted_keys(by_item)) do
  local g = by_item[k]
  print(string.format("  %-10s n=%4d stock=%6d sold=%6d instance depth=%d", k, g.count, g.stock, g.sold, g.depth))
end
print(string.format("shop revenue=%d sales=%d", revenue, sales_count))

local ranked = {}
for i, a in ipairs(accounts) do ranked[i] = a end
table.sort(ranked, function(x, y)
  if x.balance ~= y.balance then return x.balance > y.balance end
  return x.id < y.id
end)
for i = 1, 5 do
  local a = ranked[i]
  print(string.format("top %d: %-14s balance=%d last=%s", i, a.describe(a), a.balance, a.log and a.log[#a.log] or "-"))
end
local worst = ranked[#ranked]
print(string.format("lowest: %s balance=%d log=%s", worst.describe(worst), worst.balance,
  table.concat(worst.log or {}, ",")))

-- delegation checks: a lookup through the chain and an override that shadows it
local p = new_account(Premium, 4, 1000)
local q = new_account(Premium, 5, 1000)
print("premium fees:", p.fee, q.fee, "rate:", p.rate, "tax:", p.tax_rate, "withdrawals:", p.withdrawals)
print("raw own fields:", rawget(p, "fee"), rawget(q, "fee"), rawget(p, "kind"))
local total_events = 0
for _, a in ipairs(accounts) do total_events = total_events + a.events end
for _, it in ipairs(items) do total_events = total_events + it.events end
print("total audited events:", total_events)
