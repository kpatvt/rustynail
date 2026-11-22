-- docs/JIT_TRACE.md section 39.1 (T1, `retfsel`): side traces of a function-entry family pop the root frame at its RETURN when the caller's
-- loop around the call has a root trace and no recursion is popped (the lexer shape). Same output as the reference in every mode.
local src = string.rep("ab cde  f ghij kl m  nopqr 12 x", 300)

-- (1) a scanner whose loops return a token to a hot caller loop (two return sites, two result counts)
local function scan(s, p)
  while true do
    local b = s:byte(p)
    if not b then return nil, p end
    if b ~= 32 then break end
    p = p + 1
  end
  local q = p
  while true do
    local c = s:byte(q)
    if not c or c == 32 then break end
    q = q + 1
  end
  if q - p > 3 then return "long", q end
  return "short", q
end
local nl, ns, p = 0, 0, 1
for round = 1, 30 do
  p = 1
  while true do
    local kind
    kind, p = scan(src, p)
    if not kind then break end
    if kind == "long" then nl = nl + 1 else ns = ns + 1 end
  end
end
print("tokens", nl, ns)

-- (2) the same caller loop inside a function called repeatedly, with a table store and a float in the continuation
local function count(s)
  local t, q, f = {}, 1, 0.5
  while true do
    local kind
    kind, q = scan(s, q)
    if not kind then break end
    t[kind] = (t[kind] or 0) + 1
    f = f * 1.25 % 7
  end
  return t.long or 0, t.short or 0, f
end
local a, b, f = 0, 0, 0
for r = 1, 12 do
  local x, y, z = count(src)
  a, b, f = a + x, b + y, f + z
end
print("count", a, b, string.format("%.6f", f))

-- (3) an error raised in the caller after the pop, under pcall; the traceback names the caller
local function strict(s, limit)
  local q, n = 1, 0
  while true do
    local kind
    kind, q = scan(s, q)
    if not kind then break end
    n = n + 1
    if n == limit then error("limit " .. n .. " " .. kind) end
  end
  return n
end
for _, lim in ipairs({ 5000, 777, 3, 2400 }) do
  local ok, e = pcall(strict, src, lim)
  print("strict", lim, ok, (tostring(e):gsub("^.-:%d+: ", "")))
end

-- (4) the scanner driven from a coroutine that yields every 500 tokens
local co = coroutine.wrap(function()
  local q, n = 1, 0
  while true do
    local kind
    kind, q = scan(src, q)
    if not kind then break end
    n = n + 1
    if n % 500 == 0 then coroutine.yield(n, kind) end
  end
  return -1
end)
for _ = 1, 30 do
  local n, k = co()
  if n == -1 then print("co done") break end
  io.write(n, k:sub(1, 1), " ")
end
print()

-- (5) collections between the pop and the next call (the continuation allocates)
local junk = 0
for r = 1, 8 do
  local q = 1
  while true do
    local kind
    kind, q = scan(src, q)
    if not kind then break end
    local t = { kind, q }
    junk = junk + #t
    if q % 97 == 0 then collectgarbage("step") end
  end
end
print("junk", junk)
