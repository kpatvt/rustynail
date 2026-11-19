const N = Number(process.argv[2]) || 25;
const COUNT = 1000;
const ID_IDLE = 1, ID_WORKER = 2, ID_HANDLER_A = 3, ID_HANDLER_B = 4, ID_DEVICE_A = 5, ID_DEVICE_B = 6;
const KIND_DEVICE = 0, KIND_WORK = 1;
const DATA_SIZE = 4;
const floor = Math.floor;

class Packet {
  constructor(link, identity, kind) {
    this.link = link; this.identity = identity; this.kind = kind; this.datum = 0; this.data = [0, 0, 0, 0];
  }
  addTo(queue) {
    this.link = null;
    if (queue === null) return this;
    let nxt = queue;
    let peek = nxt.link;
    while (peek !== null) {
      nxt = peek;
      peek = nxt.link;
    }
    nxt.link = this;
    return queue;
  }
}

class TCB {
  constructor(link, identity, priority, queue, task) {
    this.link = link; this.identity = identity; this.priority = priority; this.input = queue; this.task = task;
    this.packetPending = false; this.taskWaiting = true; this.taskHolding = false;
    if (queue !== null) this.packetPending = true;
  }
  setRunning() { this.packetPending = false; this.taskWaiting = false; this.taskHolding = false; }
  markAsNotHeld() { this.taskHolding = false; }
  markAsHeld() { this.taskHolding = true; }
  markAsWaiting() { this.taskWaiting = true; }
  isHoldingOrWaiting() {
    return this.taskHolding || ((!this.packetPending) && this.taskWaiting);
  }
  run() {
    let message = null;
    if (this.packetPending && this.taskWaiting && !this.taskHolding) {
      message = this.input;
      this.input = message.link;
      if (this.input === null) {
        this.setRunning();
      } else {
        this.packetPending = true; this.taskWaiting = false; this.taskHolding = false;
      }
    }
    return this.task.run(message);
  }
  addInputAndCheckPriority(packet, oldTask) {
    if (this.input === null) {
      this.input = packet;
      this.packetPending = true;
      if (this.priority > oldTask.priority) return this;
    } else {
      this.input = packet.addTo(this.input);
    }
    return oldTask;
  }
}

class Scheduler {
  constructor() {
    this.taskList = null; this.currentTask = null; this.currentTaskIdentity = 0;
    this.taskTable = []; this.queueCount = 0; this.holdCount = 0;
  }
  createTask(id, priority, queue, task) {
    const tcb = new TCB(this.taskList, id, priority, queue, task);
    this.taskList = tcb;
    this.taskTable[id] = tcb;
    return tcb;
  }
  schedule() {
    this.currentTask = this.taskList;
    while (this.currentTask !== null) {
      if (this.currentTask.isHoldingOrWaiting()) {
        this.currentTask = this.currentTask.link;
      } else {
        this.currentTaskIdentity = this.currentTask.identity;
        this.currentTask = this.currentTask.run();
      }
    }
  }
  release(id) {
    const tcb = this.taskTable[id];
    if (tcb == null) return null;
    tcb.markAsNotHeld();
    if (tcb.priority > this.currentTask.priority) return tcb;
    return this.currentTask;
  }
  holdCurrent() {
    this.holdCount = this.holdCount + 1;
    this.currentTask.markAsHeld();
    return this.currentTask.link;
  }
  waitCurrent() {
    this.currentTask.markAsWaiting();
    return this.currentTask;
  }
  queue(packet) {
    const t = this.taskTable[packet.identity];
    if (t == null) return null;
    this.queueCount = this.queueCount + 1;
    packet.link = null;
    packet.identity = this.currentTaskIdentity;
    return t.addInputAndCheckPriority(packet, this.currentTask);
  }
}

class IdleTask {
  constructor(scheduler, v1, count) { this.scheduler = scheduler; this.v1 = v1; this.count = count; }
  run() {
    this.count = this.count - 1;
    if (this.count === 0) return this.scheduler.holdCurrent();
    const v1 = this.v1;
    if (v1 % 2 === 0) {
      this.v1 = floor(v1 / 2);
      return this.scheduler.release(ID_DEVICE_A);
    } else {
      // (v1 >> 1) ~ 0xD008: flip bits 15, 14, 12 and 3 of the 15-bit value
      let x = floor(v1 / 2);
      let b = floor(x / 32768) % 2; x = x + (1 - 2 * b) * 32768;
      b = floor(x / 16384) % 2; x = x + (1 - 2 * b) * 16384;
      b = floor(x / 4096) % 2; x = x + (1 - 2 * b) * 4096;
      b = floor(x / 8) % 2; x = x + (1 - 2 * b) * 8;
      this.v1 = x;
      return this.scheduler.release(ID_DEVICE_B);
    }
  }
}

class DeviceTask {
  constructor(scheduler) { this.scheduler = scheduler; this.v1 = null; }
  run(packet) {
    if (packet === null) {
      if (this.v1 === null) return this.scheduler.waitCurrent();
      const v = this.v1;
      this.v1 = null;
      return this.scheduler.queue(v);
    } else {
      this.v1 = packet;
      return this.scheduler.holdCurrent();
    }
  }
}

class WorkerTask {
  constructor(scheduler, v1, v2) { this.scheduler = scheduler; this.v1 = v1; this.v2 = v2; }
  run(packet) {
    if (packet === null) return this.scheduler.waitCurrent();
    if (this.v1 === ID_HANDLER_A) this.v1 = ID_HANDLER_B; else this.v1 = ID_HANDLER_A;
    packet.identity = this.v1;
    packet.datum = 0;
    for (let i = 0; i < DATA_SIZE; i++) {
      this.v2 = this.v2 + 1;
      if (this.v2 > 26) this.v2 = 1;
      packet.data[i] = this.v2;
    }
    return this.scheduler.queue(packet);
  }
}

class HandlerTask {
  constructor(scheduler) { this.scheduler = scheduler; this.v1 = null; this.v2 = null; }
  run(packet) {
    if (packet !== null) {
      if (packet.kind === KIND_WORK) {
        this.v1 = packet.addTo(this.v1);
      } else {
        this.v2 = packet.addTo(this.v2);
      }
    }
    if (this.v1 !== null) {
      const count = this.v1.datum;
      if (count < DATA_SIZE) {
        if (this.v2 !== null) {
          const v = this.v2;
          this.v2 = this.v2.link;
          v.datum = this.v1.data[count];
          this.v1.datum = count + 1;
          return this.scheduler.queue(v);
        }
      } else {
        const v = this.v1;
        this.v1 = this.v1.link;
        return this.scheduler.queue(v);
      }
    }
    return this.scheduler.waitCurrent();
  }
}

function runOnce() {
  const s = new Scheduler();
  const tcb = s.createTask(ID_IDLE, 0, null, new IdleTask(s, 1, COUNT));
  tcb.setRunning();
  let queue = new Packet(null, ID_WORKER, KIND_WORK);
  queue = new Packet(queue, ID_WORKER, KIND_WORK);
  s.createTask(ID_WORKER, 1000, queue, new WorkerTask(s, ID_HANDLER_A, 0));
  queue = new Packet(null, ID_DEVICE_A, KIND_DEVICE);
  queue = new Packet(queue, ID_DEVICE_A, KIND_DEVICE);
  queue = new Packet(queue, ID_DEVICE_A, KIND_DEVICE);
  s.createTask(ID_HANDLER_A, 2000, queue, new HandlerTask(s));
  queue = new Packet(null, ID_DEVICE_B, KIND_DEVICE);
  queue = new Packet(queue, ID_DEVICE_B, KIND_DEVICE);
  queue = new Packet(queue, ID_DEVICE_B, KIND_DEVICE);
  s.createTask(ID_HANDLER_B, 3000, queue, new HandlerTask(s));
  s.createTask(ID_DEVICE_A, 4000, null, new DeviceTask(s));
  s.createTask(ID_DEVICE_B, 5000, null, new DeviceTask(s));
  s.schedule();
  return [s.queueCount, s.holdCount];
}

let ok = 0;
let q, h;
for (let i = 1; i <= N; i++) {
  [q, h] = runOnce();
  if (q === 2322 && h === 928) ok = ok + 1;
}
console.log(`richards\t${ok}\t${q}\t${h}`);
