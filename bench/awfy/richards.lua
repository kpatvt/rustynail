-- Richards: OS kernel task scheduler simulation (port of the public are-we-fast-yet "Richards" description:
-- tasks of four classes (idle, worker, handler, device) scheduled by priority, passing packets).
-- Plain Lua 5.1-compatible: runs unchanged on PUC 5.4/5.5 and LuaJIT (no bit operators: the idle task's
-- 16-bit LFSR step is done with arithmetic).  Verification: queueCount == 2322 and holdCount == 928 per run.
local N = tonumber(arg and arg[1]) or 25
local COUNT = 1000
local ID_IDLE, ID_WORKER, ID_HANDLER_A, ID_HANDLER_B, ID_DEVICE_A, ID_DEVICE_B = 1, 2, 3, 4, 5, 6
local KIND_DEVICE, KIND_WORK = 0, 1
local DATA_SIZE = 4
local floor = math.floor

-- Packet
local Packet = {}
Packet.__index = Packet
local function newPacket(link, identity, kind)
  return setmetatable({ link = link, identity = identity, kind = kind, datum = 0, data = { 0, 0, 0, 0 } }, Packet)
end
function Packet:addTo(queue)
  self.link = nil
  if queue == nil then return self end
  local nxt = queue
  local peek = nxt.link
  while peek ~= nil do
    nxt = peek
    peek = nxt.link
  end
  nxt.link = self
  return queue
end

-- Task control block
local TCB = {}
TCB.__index = TCB
local function newTCB(link, identity, priority, queue, task)
  local t = setmetatable({ link = link, identity = identity, priority = priority, input = queue, task = task,
    packetPending = false, taskWaiting = true, taskHolding = false }, TCB)
  if queue ~= nil then t.packetPending = true end
  return t
end
function TCB:setRunning() self.packetPending = false; self.taskWaiting = false; self.taskHolding = false end
function TCB:markAsNotHeld() self.taskHolding = false end
function TCB:markAsHeld() self.taskHolding = true end
function TCB:markAsWaiting() self.taskWaiting = true end
function TCB:isHoldingOrWaiting()
  return self.taskHolding or ((not self.packetPending) and self.taskWaiting)
end
function TCB:run()
  local message
  if self.packetPending and self.taskWaiting and not self.taskHolding then
    message = self.input
    self.input = message.link
    if self.input == nil then
      self:setRunning()
    else
      self.packetPending = true; self.taskWaiting = false; self.taskHolding = false
    end
  end
  return self.task:run(message)
end
function TCB:addInputAndCheckPriority(packet, oldTask)
  if self.input == nil then
    self.input = packet
    self.packetPending = true
    if self.priority > oldTask.priority then return self end
  else
    self.input = packet:addTo(self.input)
  end
  return oldTask
end

-- Scheduler
local Scheduler = {}
Scheduler.__index = Scheduler
local function newScheduler()
  return setmetatable({ taskList = nil, currentTask = nil, currentTaskIdentity = 0, taskTable = {}, queueCount = 0, holdCount = 0 }, Scheduler)
end
function Scheduler:createTask(id, priority, queue, task)
  local tcb = newTCB(self.taskList, id, priority, queue, task)
  self.taskList = tcb
  self.taskTable[id] = tcb
  return tcb
end
function Scheduler:schedule()
  self.currentTask = self.taskList
  while self.currentTask ~= nil do
    if self.currentTask:isHoldingOrWaiting() then
      self.currentTask = self.currentTask.link
    else
      self.currentTaskIdentity = self.currentTask.identity
      self.currentTask = self.currentTask:run()
    end
  end
end
function Scheduler:release(id)
  local tcb = self.taskTable[id]
  if tcb == nil then return tcb end
  tcb:markAsNotHeld()
  if tcb.priority > self.currentTask.priority then return tcb end
  return self.currentTask
end
function Scheduler:holdCurrent()
  self.holdCount = self.holdCount + 1
  self.currentTask:markAsHeld()
  return self.currentTask.link
end
function Scheduler:waitCurrent()
  self.currentTask:markAsWaiting()
  return self.currentTask
end
function Scheduler:queue(packet)
  local t = self.taskTable[packet.identity]
  if t == nil then return t end
  self.queueCount = self.queueCount + 1
  packet.link = nil
  packet.identity = self.currentTaskIdentity
  return t:addInputAndCheckPriority(packet, self.currentTask)
end

-- Tasks
local IdleTask = {}
IdleTask.__index = IdleTask
function IdleTask:run()
  self.count = self.count - 1
  if self.count == 0 then return self.scheduler:holdCurrent() end
  local v1 = self.v1
  if v1 % 2 == 0 then
    self.v1 = floor(v1 / 2)
    return self.scheduler:release(ID_DEVICE_A)
  else
    -- (v1 >> 1) ~ 0xD008: flip bits 15, 14, 12 and 3 of the 15-bit value
    local x = floor(v1 / 2)
    local b = floor(x / 32768) % 2; x = x + (1 - 2 * b) * 32768
    b = floor(x / 16384) % 2; x = x + (1 - 2 * b) * 16384
    b = floor(x / 4096) % 2; x = x + (1 - 2 * b) * 4096
    b = floor(x / 8) % 2; x = x + (1 - 2 * b) * 8
    self.v1 = x
    return self.scheduler:release(ID_DEVICE_B)
  end
end

local DeviceTask = {}
DeviceTask.__index = DeviceTask
function DeviceTask:run(packet)
  if packet == nil then
    if self.v1 == nil then return self.scheduler:waitCurrent() end
    local v = self.v1
    self.v1 = nil
    return self.scheduler:queue(v)
  else
    self.v1 = packet
    return self.scheduler:holdCurrent()
  end
end

local WorkerTask = {}
WorkerTask.__index = WorkerTask
function WorkerTask:run(packet)
  if packet == nil then return self.scheduler:waitCurrent() end
  if self.v1 == ID_HANDLER_A then self.v1 = ID_HANDLER_B else self.v1 = ID_HANDLER_A end
  packet.identity = self.v1
  packet.datum = 0
  for i = 1, DATA_SIZE do
    self.v2 = self.v2 + 1
    if self.v2 > 26 then self.v2 = 1 end
    packet.data[i] = self.v2
  end
  return self.scheduler:queue(packet)
end

local HandlerTask = {}
HandlerTask.__index = HandlerTask
function HandlerTask:run(packet)
  if packet ~= nil then
    if packet.kind == KIND_WORK then
      self.v1 = packet:addTo(self.v1)
    else
      self.v2 = packet:addTo(self.v2)
    end
  end
  if self.v1 ~= nil then
    local count = self.v1.datum
    if count < DATA_SIZE then
      if self.v2 ~= nil then
        local v = self.v2
        self.v2 = self.v2.link
        v.datum = self.v1.data[count + 1]
        self.v1.datum = count + 1
        return self.scheduler:queue(v)
      end
    else
      local v = self.v1
      self.v1 = self.v1.link
      return self.scheduler:queue(v)
    end
  end
  return self.scheduler:waitCurrent()
end

local function runOnce()
  local s = newScheduler()
  local tcb = s:createTask(ID_IDLE, 0, nil, setmetatable({ scheduler = s, v1 = 1, count = COUNT }, IdleTask))
  tcb:setRunning()
  local queue = newPacket(nil, ID_WORKER, KIND_WORK)
  queue = newPacket(queue, ID_WORKER, KIND_WORK)
  s:createTask(ID_WORKER, 1000, queue, setmetatable({ scheduler = s, v1 = ID_HANDLER_A, v2 = 0 }, WorkerTask))
  queue = newPacket(nil, ID_DEVICE_A, KIND_DEVICE)
  queue = newPacket(queue, ID_DEVICE_A, KIND_DEVICE)
  queue = newPacket(queue, ID_DEVICE_A, KIND_DEVICE)
  s:createTask(ID_HANDLER_A, 2000, queue, setmetatable({ scheduler = s }, HandlerTask))
  queue = newPacket(nil, ID_DEVICE_B, KIND_DEVICE)
  queue = newPacket(queue, ID_DEVICE_B, KIND_DEVICE)
  queue = newPacket(queue, ID_DEVICE_B, KIND_DEVICE)
  s:createTask(ID_HANDLER_B, 3000, queue, setmetatable({ scheduler = s }, HandlerTask))
  s:createTask(ID_DEVICE_A, 4000, nil, setmetatable({ scheduler = s }, DeviceTask))
  s:createTask(ID_DEVICE_B, 5000, nil, setmetatable({ scheduler = s }, DeviceTask))
  s:schedule()
  return s.queueCount, s.holdCount
end

local ok = 0
local q, h
for i = 1, N do
  q, h = runOnce()
  if q == 2322 and h == 928 then ok = ok + 1 end
end
print("richards", ok, q, h)
