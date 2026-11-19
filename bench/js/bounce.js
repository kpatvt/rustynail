const N = Number(process.argv[2]) || 350;
const abs = Math.abs;
class Random {
  constructor() { this.seed = 74755; }
  next() { this.seed = (this.seed * 1309 + 13849) % 65536; return this.seed; }
}
class Ball {
  constructor(random) {
    this.x = random.next() % 500; this.y = random.next() % 500;
    this.xVel = (random.next() % 300) - 150; this.yVel = (random.next() % 300) - 150;
  }
  bounce() {
    const xLimit = 500, yLimit = 500;
    let bounced = false;
    this.x = this.x + this.xVel;
    this.y = this.y + this.yVel;
    if (this.x > xLimit) { this.x = xLimit; this.xVel = 0 - abs(this.xVel); bounced = true; }
    if (this.x < 0) { this.x = 0; this.xVel = abs(this.xVel); bounced = true; }
    if (this.y > yLimit) { this.y = yLimit; this.yVel = 0 - abs(this.yVel); bounced = true; }
    if (this.y < 0) { this.y = 0; this.yVel = abs(this.yVel); bounced = true; }
    return bounced;
  }
}
let total = 0, ok = 0;
for (let it = 1; it <= N; it++) {
  const random = new Random();
  const balls = [];
  for (let i = 0; i < 100; i++) balls.push(new Ball(random));
  let bounces = 0;
  for (let i = 1; i <= 50; i++)
    for (let j = 0; j < 100; j++)
      if (balls[j].bounce()) bounces = bounces + 1;
  total = total + bounces;
  if (bounces === 1331) ok = ok + 1;
}
console.log(`bounce\t${ok}\t${total}`);
