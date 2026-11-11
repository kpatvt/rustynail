import re, sys
p = sys.argv[1]; s = open(p).read()
n0 = s.count('~>>')
s, n = re.subn(r'\(op << ([0-9a-z+\-]+)\) ~>> ([0-9a-z+\-]+)', r'sar32(op << (\1), \2)', s)
assert n == n0, (n, n0)
anchor = re.search(r'local [a-z, ]*tohex = bit[.a-z, ]*tohex\n', s).group(0)
s = s.replace(anchor, anchor + '''
-- [corpus2 port] the dialect's `x ~>> n` (arithmetic shift of a 32-bit value) as a function.
local function sar32(v, n)
  v = v & 0xffffffff
  if v >= 0x80000000 then v = v - 0x100000000 end
  return math.floor(v / (1 << n))
end
''', 1)
open(p, 'w').write(s)
