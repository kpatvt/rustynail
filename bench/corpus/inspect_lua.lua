-- Derived from inspect.lua (https://github.com/kikito/inspect.lua), Copyright (c) Enrique García Cota, MIT License; see THIRD_PARTY.md. Sorted keys by type order, cycle ids, smart quoting,
-- metatables, depth limit, process callback; workload pretty-prints a set of nested structures many times.
local inspect = {}
inspect.KEY = setmetatable({}, { __tostring = function() return 'inspect.KEY' end })
inspect.METATABLE = setmetatable({}, { __tostring = function() return 'inspect.METATABLE' end })

local tostring = tostring
local rep = string.rep
local match = string.match
local char = string.char
local gsub = string.gsub
local fmt = string.format

local function rawpairs(t) return next, t, nil end

local function smartQuote(str)
  if match(str, '"') and not match(str, "'") then
    return "'" .. str .. "'"
  end
  return '"' .. gsub(str, '"', '\\"') .. '"'
end

local shortControlCharEscapes = {
  ["\a"] = "\\a", ["\b"] = "\\b", ["\f"] = "\\f", ["\n"] = "\\n",
  ["\r"] = "\\r", ["\t"] = "\\t", ["\v"] = "\\v", ["\127"] = "\\127",
}
local longControlCharEscapes = { ["\127"] = "\127" }
for i = 0, 31 do
  local ch = char(i)
  if not shortControlCharEscapes[ch] then
    shortControlCharEscapes[ch] = "\\" .. i
    longControlCharEscapes[ch] = fmt("\\%03d", i)
  end
end

local function escape(str)
  return (gsub(gsub(gsub(str, "\\", "\\\\"), "(%c)%f[0-9]", longControlCharEscapes), "%c", shortControlCharEscapes))
end

local luaKeywords = {}
for w in ("and break do else elseif end false for function goto if in local nil not or repeat return then true until while"):gmatch("%a+") do
  luaKeywords[w] = true
end
local function isIdentifier(str)
  return type(str) == "string" and not not str:match("^[_%a][_%w]*$") and not luaKeywords[str]
end

local flr = math.floor
local function isSequenceKey(k, sequenceLength)
  return type(k) == "number" and flr(k) == k and 1 <= (k) and k <= sequenceLength
end

local defaultTypeOrders = {
  ['number'] = 1, ['boolean'] = 2, ['string'] = 3, ['table'] = 4,
  ['function'] = 5, ['userdata'] = 6, ['thread'] = 7,
}

local function sortKeys(a, b)
  local ta, tb = type(a), type(b)
  -- strings and numbers are sorted numerically/alphabetically
  if ta == tb and (ta == 'string' or ta == 'number') then
    return (a) < (b)
  end
  local dta = defaultTypeOrders[ta] or 100
  local dtb = defaultTypeOrders[tb] or 100
  -- Two default types are compared according to the defaultTypeOrders table
  return dta == dtb and ta < tb or dta < dtb
end

local function getKeys(t)
  local seqLen = 1
  while rawget(t, seqLen) ~= nil do
    seqLen = seqLen + 1
  end
  seqLen = seqLen - 1

  local keys, keysLen = {}, 0
  for k in rawpairs(t) do
    if not isSequenceKey(k, seqLen) then
      keysLen = keysLen + 1
      keys[keysLen] = k
    end
  end
  table.sort(keys, sortKeys)
  return keys, keysLen, seqLen
end

local function countCycles(x, cycles)
  if type(x) == "table" then
    if cycles[x] then
      cycles[x] = cycles[x] + 1
    else
      cycles[x] = 1
      for k, v in rawpairs(x) do
        countCycles(k, cycles)
        countCycles(v, cycles)
      end
      countCycles(getmetatable(x), cycles)
    end
  end
end

local function makePath(path, a, b)
  local newPath = {}
  local len = #path
  for i = 1, len do newPath[i] = path[i] end
  newPath[len + 1] = a
  newPath[len + 2] = b
  return newPath
end

local function processRecursive(process, item, path, visited)
  if item == nil then return nil end
  if visited[item] then return visited[item] end

  local processed = process(item, path)
  if type(processed) == "table" then
    local processedCopy = {}
    visited[item] = processedCopy
    local processedKey

    for k, v in rawpairs(processed) do
      processedKey = processRecursive(process, k, makePath(path, k, inspect.KEY), visited)
      if processedKey ~= nil then
        processedCopy[processedKey] = processRecursive(process, v, makePath(path, processedKey), visited)
      end
    end

    local mt = processRecursive(process, getmetatable(processed), makePath(path, inspect.METATABLE), visited)
    if type(mt) ~= 'table' then mt = nil end
    setmetatable(processedCopy, mt)
    processed = processedCopy
  end
  return processed
end

local function puts(buf, str)
  buf.n = buf.n + 1
  buf[buf.n] = str
end

local Inspector = {}
local Inspector_mt = { __index = Inspector }

local function tabify(inspector)
  puts(inspector.buf, inspector.newline .. rep(inspector.indent, inspector.level))
end

function Inspector:getId(v)
  local id = self.ids[v]
  local ids = self.ids
  if not id then
    local tv = type(v)
    id = (ids[tv] or 0) + 1
    ids[v], ids[tv] = id, id
  end
  return tostring(id)
end

function Inspector:putValue(v)
  local buf = self.buf
  local tv = type(v)
  if tv == 'string' then
    puts(buf, smartQuote(escape(v)))
  elseif tv == 'number' or tv == 'boolean' or tv == 'nil' or
    tv == 'cdata' or tv == 'ctype' then
    puts(buf, tostring(v))
  elseif tv == 'table' and not self.ids[v] then
    local t = v

    if t == inspect.KEY or t == inspect.METATABLE then
      puts(buf, tostring(t))
    elseif self.level >= self.depth then
      puts(buf, '{...}')
    else
      if self.cycles[t] > 1 then puts(buf, fmt('<%d>', self:getId(t))) end

      local keys, keysLen, seqLen = getKeys(t)

      puts(buf, '{')
      self.level = self.level + 1

      for i = 1, seqLen + keysLen do
        if i > 1 then puts(buf, ',') end
        if i <= seqLen then
          puts(buf, ' ')
          self:putValue(t[i])
        else
          local k = keys[i - seqLen]
          tabify(self)
          if isIdentifier(k) then
            puts(buf, k)
          else
            puts(buf, "[")
            self:putValue(k)
            puts(buf, "]")
          end
          puts(buf, ' = ')
          self:putValue(t[k])
        end
      end

      local mt = getmetatable(t)
      if type(mt) == 'table' then
        if seqLen + keysLen > 0 then puts(buf, ',') end
        tabify(self)
        puts(buf, '<metatable> = ')
        self:putValue(mt)
      end

      self.level = self.level - 1

      if keysLen > 0 or type(mt) == 'table' then
        tabify(self)
      elseif seqLen > 0 then
        puts(buf, ' ')
      end

      puts(buf, '}')
    end

  else
    puts(buf, fmt('<%s %d>', tv, self:getId(v)))
  end
end

function inspect.inspect(root, options)
  options = options or {}

  local depth = options.depth or (math.huge)
  local newline = options.newline or '\n'
  local indent = options.indent or '  '
  local process = options.process

  if process then
    root = processRecursive(process, root, {}, {})
  end

  local cycles = {}
  countCycles(root, cycles)

  local inspector = setmetatable({
    buf = { n = 0 },
    ids = {},
    cycles = cycles,
    depth = depth,
    level = 0,
    newline = newline,
    indent = indent,
  }, Inspector_mt)

  inspector:putValue(root)

  return table.concat(inspector.buf)
end

setmetatable(inspect, { __call = function(_, root, options) return inspect.inspect(root, options) end })

-- Workload
local N = tonumber(arg and arg[1]) or 1000
local function build(i)
  local t = {
    name = "node" .. i, ["with space"] = i, [true] = "yes", [1.5] = "float key",
    list = { 1, 2, 3, "four", { five = 5 } },
    text = "line1\nline2\t\"quoted\" 'single' \1\2 end",
    ["end"] = "keyword key",
    nested = { a = { b = { c = { d = { e = i } } } } },
  }
  for k = 1, 10 do t[k] = k * i end
  t.self = t
  t.list[6] = t.nested
  setmetatable(t.nested, { __index = { inherited = true }, __mode = "k" })
  return t
end
local data = {}
for i = 1, 30 do data[i] = build(i) end

local total, h = 0, 0
local s
for r = 1, N do
  s = inspect(data[1 + r % #data])
  total = total + #s
  local s2 = inspect(data, { depth = 2, newline = " ", indent = "" })
  total = total + #s2
  if r % 10 == 0 then
    local s3 = inspect(data[1 + r % #data], { process = function(item, path)
      if path[#path] ~= inspect.METATABLE and type(item) ~= "function" then return item end
    end })
    total = total + #s3
  end
  for i = 1, #s, 5 do h = (h * 31 + s:byte(i)) % 1000000007 end
end
print("total", total, "hash", h)
print(s:sub(1, 600))
