function BottomUpTree(depth) {
  if (depth > 0) { depth = depth - 1; const left = BottomUpTree(depth), right = BottomUpTree(depth); return [left, right]; }
  else return [];
}
function ItemCheck(tree) { if (tree[0]) return 1 + ItemCheck(tree[0]) + ItemCheck(tree[1]); else return 1; }
const N = Number(process.argv[2]) || 16;
const mindepth = 4;
let maxdepth = mindepth + 2;
if (maxdepth < N) maxdepth = N;
{ const stretchdepth = maxdepth + 1; console.log(`stretch tree of depth ${stretchdepth}\t check: ${ItemCheck(BottomUpTree(stretchdepth))}`); }
const longlivedtree = BottomUpTree(maxdepth);
for (let depth = mindepth; depth <= maxdepth; depth += 2) {
  const iterations = 2 ** (maxdepth - depth + mindepth);
  let check = 0;
  for (let i = 1; i <= iterations; i++) check = check + ItemCheck(BottomUpTree(depth));
  console.log(`${iterations}\t trees of depth ${depth}\t check: ${check}`);
}
console.log(`long lived tree of depth ${maxdepth}\t check: ${ItemCheck(longlivedtree)}`);
