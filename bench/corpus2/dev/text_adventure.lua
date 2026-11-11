-- Text adventure engine: rooms, items, inventory, verb/noun parser with synonyms, NPC dialogue
-- state machine, driven by a deterministic generated command script. small: N=3
local N = tonumber(arg and arg[1]) or 800000

local seed = 4242
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- World definition ---------------------------------------------------------
local rooms = {
  hall    = { desc = "A grand hall.", exits = { north = "library", east = "kitchen", down = "cellar" }, items = { "lamp" } },
  library = { desc = "Dusty shelves line the walls.", exits = { south = "hall", east = "study" }, items = { "book", "scroll" } },
  kitchen = { desc = "Pots hang from hooks.", exits = { west = "hall", north = "study" }, items = { "knife", "bread" } },
  study   = { desc = "A quiet study.", exits = { west = "library", south = "kitchen" }, items = { "key" }, npc = "wizard" },
  cellar  = { desc = "Cold and damp.", exits = { up = "hall" }, items = { "coin" }, locked = true },
}

local itemWeights = { lamp = 5, book = 3, scroll = 1, knife = 2, bread = 1, key = 1, coin = 1, potion = 1 }

local verbs = {
  go = "go", walk = "go", move = "go", run = "go",
  take = "take", get = "take", grab = "take", pick = "take",
  drop = "drop", leave = "drop", discard = "drop",
  look = "look", examine = "look", inspect = "look", l = "look",
  inventory = "inv", inv = "inv", i = "inv",
  talk = "talk", speak = "talk", chat = "talk",
  use = "use", eat = "use", read = "use",
  unlock = "unlock", open = "unlock",
}
local directions = { n = "north", s = "south", e = "east", w = "west", u = "up", d = "down",
  north = "north", south = "south", east = "east", west = "west", up = "up", down = "down" }

-- NPC dialogue state machine ----------------------------------------------
local dialogue = {
  start   = { say = "Wizard: Greetings, traveller.", next = "ask" },
  ask     = { say = "Wizard: Do you bring me a book?", next = "waiting", needs = "book" },
  waiting = { say = "Wizard: I am still waiting.", next = "waiting", needs = "book", on_ok = "reward" },
  reward  = { say = "Wizard: Splendid! Take this potion.", next = "done", gives = "potion" },
  done    = { say = "Wizard: Farewell.", next = "done" },
}

local Game = {}
Game.__index = Game

function Game.new()
  local g = setmetatable({}, Game)
  g.rooms = {}
  for name, r in pairs(rooms) do
    local copy = { desc = r.desc, exits = r.exits, items = {}, npc = r.npc, locked = r.locked }
    for _, it in ipairs(r.items) do copy.items[#copy.items + 1] = it end
    g.rooms[name] = copy
  end
  g.at = "hall"
  g.inv = {}
  g.score = 0
  g.moves = 0
  g.talk = "start"
  g.log = {}
  g.counts = {}
  return g
end

function Game:say(fmt, ...)
  local line = string.format(fmt, ...)
  self.log[#self.log + 1] = line
  self.lines = (self.lines or 0) + 1
  if #self.log > 3 then table.remove(self.log, 1) end
  self.chars = (self.chars or 0) + #line
end

local function indexOf(list, v)
  for i, x in ipairs(list) do if x == v then return i end end
end

function Game:has(item) return indexOf(self.inv, item) ~= nil end

function Game:parse(line)
  local words = {}
  for w in line:lower():gmatch("%a+") do
    if w ~= "the" and w ~= "to" and w ~= "a" then words[#words + 1] = w end
  end
  local verb = verbs[words[1]]
  if not verb and directions[words[1]] then
    return "go", directions[words[1]]
  end
  local noun = words[2]
  if verb == "go" and noun then noun = directions[noun] end
  return verb, noun
end

local handlers = {}

function handlers.go(g, dir)
  local room = g.rooms[g.at]
  local dest = dir and room.exits[dir]
  if not dest then return g:say("You can't go that way.") end
  if g.rooms[dest].locked then return g:say("The door to the %s is locked.", dest) end
  g.at = dest
  g.moves = g.moves + 1
  g:say("You walk %s to the %s. %s", dir, dest, g.rooms[dest].desc)
end

function handlers.take(g, item)
  local room = g.rooms[g.at]
  local idx = item and indexOf(room.items, item)
  if not idx then return g:say("There is no %s here.", tostring(item)) end
  if #g.inv >= 4 then return g:say("Your hands are full.") end
  table.remove(room.items, idx)
  g.inv[#g.inv + 1] = item
  g.score = g.score + (itemWeights[item] or 0)
  g:say("Taken: %s.", item)
end

function handlers.drop(g, item)
  local idx = item and indexOf(g.inv, item)
  if not idx then return g:say("You don't have %s.", tostring(item)) end
  table.remove(g.inv, idx)
  local room = g.rooms[g.at]
  room.items[#room.items + 1] = item
  g.score = g.score - (itemWeights[item] or 0)
  g:say("Dropped: %s.", item)
end

function handlers.look(g)
  local room = g.rooms[g.at]
  local exits = {}
  for dir in pairs(room.exits) do exits[#exits + 1] = dir end
  table.sort(exits)
  g:say("%s Items: %s. Exits: %s.", room.desc, table.concat(room.items, ","), table.concat(exits, ","))
end

function handlers.inv(g)
  local sorted = {}
  for i, v in ipairs(g.inv) do sorted[i] = v end
  table.sort(sorted)
  g:say("You carry: %s", #sorted > 0 and table.concat(sorted, ", ") or "nothing")
end

function handlers.talk(g)
  local room = g.rooms[g.at]
  if not room.npc then return g:say("Nobody is here to talk to.") end
  local node = dialogue[g.talk]
  g:say(node.say)
  local nextState = node.next
  if node.needs and g:has(node.needs) then
    nextState = node.on_ok or "reward"
  end
  if node.gives then
    g.inv[#g.inv + 1] = node.gives
    g.score = g.score + 10
  end
  g.talk = nextState
end

function handlers.use(g, item)
  if not (item and g:has(item)) then return g:say("You can't use that.") end
  if item == "bread" then
    table.remove(g.inv, indexOf(g.inv, "bread"))
    g.score = g.score + 2
    g:say("You eat the bread.")
  else
    g:say("Nothing happens with the %s.", item)
  end
end

function handlers.unlock(g, what)
  if g.at == "hall" and g:has("key") and g.rooms.cellar.locked then
    g.rooms.cellar.locked = false
    g.score = g.score + 5
    g:say("The cellar door clicks open.")
  else
    g:say("You can't unlock that.")
  end
end

function Game:exec(line)
  local verb, noun = self:parse(line)
  if not verb then return self:say("I don't understand '%s'.", line) end
  self.counts[verb] = (self.counts[verb] or 0) + 1
  handlers[verb](self, noun)
end

-- Script generation ---------------------------------------------------------
local verbWords = {}
for w in pairs(verbs) do verbWords[#verbWords + 1] = w end
table.sort(verbWords)
local itemNames = {}
for w in pairs(itemWeights) do itemNames[#itemNames + 1] = w end
table.sort(itemNames)
local dirNames = { "north", "south", "east", "west", "up", "down", "n", "e", "s", "w" }

local function genCommand()
  local r = rnd(10)
  if r <= 4 then return (rnd(2) == 1 and "go " or "") .. dirNames[rnd(#dirNames)]
  elseif r <= 6 then return verbWords[rnd(#verbWords)] .. " the " .. itemNames[rnd(#itemNames)]
  elseif r == 7 then return "talk to wizard"
  elseif r == 8 then return "unlock door"
  elseif r == 9 then return "Look"
  else return "dance wildly" end
end

local g = Game.new()
g:exec("take lamp")
for i = 1, N do
  local cmd = genCommand()
  g:exec(cmd)
  if i % math.max(1, math.floor(N / 8)) == 0 then
    print(string.format("[%4d] %-22s at=%-8s score=%d", i, cmd, g.at, g.score))
  end
end

print("last 3 log lines:")
for i = math.max(1, #g.log - 2), #g.log do print("  " .. g.log[i]) end
local names = {}
for v in pairs(g.counts) do names[#names + 1] = v end
table.sort(names)
for _, v in ipairs(names) do print(string.format("%-8s %d", v, g.counts[v])) end
print(string.format("moves=%d score=%d lines=%d chars=%d talk=%s", g.moves, g.score, g.lines, g.chars, g.talk))
