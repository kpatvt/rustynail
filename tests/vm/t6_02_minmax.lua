-- docs/JIT_TRACE.md section 44.1 (T6, `t6` bit 1): `math.min` / `math.max` of one integer and one float inline in tier 1. The result is the winning operand itself
-- (its own type); NaN never wins; integers beyond 2^53 compare exactly (generic path). Same output as the reference in every mode.
local min, max = math.min, math.max
local mtype = math.type
local nan = 0 / 0
local vals = {
  0, 1, -1, 2, -2, 7, 1000, -1000, 0.0, -0.0, 0.5, -0.5, 1.0, -1.0, 2.5, 1e15, -1e15, 1e300, -1e300, 1 / 0, -1 / 0, nan,
  9007199254740991, 9007199254740992, 9007199254740993, -9007199254740992, -9007199254740993, 9007199254740992.0, 9007199254740994.0,
  4611686018427387904, math.maxinteger, math.mininteger, 2 ^ 53, 2 ^ 63, -2 ^ 63, 2 ^ 62,
}
local function fmt(v)
  if v ~= v then return "nan" end
  return mtype(v) .. ":" .. string.format("%.17g", v)
end
-- every pair, straight-line code per call so that the calls run in tier 1 (and in traces in the other modes)
local function pair(a, b)
  return fmt(min(a, b)) .. " " .. fmt(max(a, b))
end
for i = 1, #vals do
  local row = {}
  for j = 1, #vals do row[#row + 1] = pair(vals[i], vals[j]) end
  print(i, table.concat(row, "|"))
end
-- the shapes of the benchmark: min(1, f), max(0, f), floors of them
local acc = 0
for r = 1, 300 do
  local f = (r % 7 - 3) / 2
  local a = min(1, f)
  local b = max(0, f)
  local c = min(f, 1)
  local d = max(f, 0)
  acc = acc + a + b + c + d
end
print("shape", acc, mtype(min(1, 0.5)), mtype(min(1, 2.5)), mtype(max(0, -0.5)), mtype(max(0, 0.5)))
-- a NaN and an integer, both orders, and negative zero against zero
print("nan", fmt(min(1, nan)), fmt(min(nan, 1)), fmt(max(1, nan)), fmt(max(nan, 1)))
print("zero", fmt(min(0, -0.0)), fmt(min(-0.0, 0)), fmt(max(0, -0.0)), fmt(max(-0.0, 0)), fmt(min(0, 0.0)), fmt(max(0.0, 0)))
-- the same call sites see integers, floats and both
local t = { 1, 2.5, 3, 0.25, -7, -7.5 }
local out = {}
for r = 1, 120 do
  local x, y = t[r % 6 + 1], t[(r * 5) % 6 + 1]
  out[#out + 1] = fmt(max(x, y)) .. fmt(min(x, y))
end
print("mix", table.concat(out, ","))
-- errors are the generic path's
print("err", pcall(min, 1, "x"), pcall(max, 2.5, {}))
