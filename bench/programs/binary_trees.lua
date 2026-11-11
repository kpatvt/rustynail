local function BottomUpTree(depth)
  if depth > 0 then depth = depth - 1; local left, right = BottomUpTree(depth), BottomUpTree(depth); return { left, right }
  else return {} end
end
local function ItemCheck(tree) if tree[1] then return 1 + ItemCheck(tree[1]) + ItemCheck(tree[2]) else return 1 end end
local N = tonumber(arg and arg[1]) or 16
local mindepth = 4
local maxdepth = mindepth + 2
if maxdepth < N then maxdepth = N end
do local stretchdepth = maxdepth + 1; print(string.format("stretch tree of depth %d\t check: %d", stretchdepth, ItemCheck(BottomUpTree(stretchdepth)))) end
local longlivedtree = BottomUpTree(maxdepth)
for depth = mindepth, maxdepth, 2 do
  local iterations = 2 ^ (maxdepth - depth + mindepth)
  local check = 0
  for i = 1, iterations do check = check + ItemCheck(BottomUpTree(depth)) end
  print(string.format("%d\t trees of depth %d\t check: %d", iterations, depth, check))
end
print(string.format("long lived tree of depth %d\t check: %d", maxdepth, ItemCheck(longlivedtree)))
