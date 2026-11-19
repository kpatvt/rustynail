const N = Number(process.argv[2]) || 45;
class Disk { constructor(size) { this.size = size; this.nxt = null; } }
class Towers {
  constructor() { this.piles = []; this.movesDone = 0; }
  pushDisk(disk, pile) {
    const top = this.piles[pile];
    if (top != null && disk.size >= top.size) throw new Error("Cannot put a big disk on a smaller one");
    disk.nxt = top === undefined ? null : top;
    this.piles[pile] = disk;
  }
  popDiskFrom(pile) {
    const top = this.piles[pile];
    if (top == null) throw new Error("Attempting to remove a disk from an empty pile");
    this.piles[pile] = top.nxt;
    top.nxt = null;
    return top;
  }
  moveTopDisk(from, to) {
    this.pushDisk(this.popDiskFrom(from), to);
    this.movesDone = this.movesDone + 1;
  }
  buildTowerAt(pile, disks) {
    for (let i = disks; i >= 0; i--) this.pushDisk(new Disk(i), pile);
  }
  moveDisks(disks, from, to) {
    if (disks === 1) {
      this.moveTopDisk(from, to);
    } else {
      const other = 3 - from - to;
      this.moveDisks(disks - 1, from, other);
      this.moveTopDisk(from, to);
      this.moveDisks(disks - 1, other, to);
    }
  }
}
let total = 0, ok = 0;
for (let it = 1; it <= N; it++) {
  const t = new Towers();
  t.buildTowerAt(1, 13);
  t.moveDisks(13, 1, 2);
  total = total + t.movesDone;
  if (t.movesDone === 8191) ok = ok + 1;
}
console.log(`towers\t${ok}\t${total}`);
