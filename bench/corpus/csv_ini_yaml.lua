-- Config/data parsers in the style of ftcsv / lua-csv (quoted fields, embedded separators/newlines, header maps),
-- inifile.lua (sections, comments, typed values) and a tinyyaml-like indentation parser (maps, lists, scalars).
local sfind, ssub, sbyte, sgsub, smatch = string.find, string.sub, string.byte, string.gsub, string.match

-- CSV ---------------------------------------------------------------------
local function parse_csv(input, delimiter, headers_wanted)
  delimiter = delimiter or ","
  local dbyte = sbyte(delimiter)
  local rows = {}
  local pos, len = 1, #input
  local row, field = {}, nil
  while pos <= len do
    local c = sbyte(input, pos)
    if c == 34 then -- quoted field
      local buf, bn = {}, 0
      local i = pos + 1
      while true do
        local q = sfind(input, '"', i, true)
        if not q then error("unterminated quote") end
        bn = bn + 1; buf[bn] = ssub(input, i, q - 1)
        if sbyte(input, q + 1) == 34 then
          bn = bn + 1; buf[bn] = '"'
          i = q + 2
        else
          pos = q + 1
          break
        end
      end
      field = table.concat(buf)
      c = sbyte(input, pos)
    else
      local s, e = sfind(input, "[" .. delimiter .. "\r\n]", pos)
      if s then
        field = ssub(input, pos, s - 1)
        pos = s
        c = sbyte(input, pos)
      else
        field = ssub(input, pos)
        pos = len + 1
        c = nil
      end
    end
    row[#row + 1] = field
    if c == dbyte then
      pos = pos + 1
      if pos > len then row[#row + 1] = "" end
    elseif c == 13 or c == 10 or c == nil then
      if c == 13 and sbyte(input, pos + 1) == 10 then pos = pos + 1 end
      pos = pos + 1
      rows[#rows + 1] = row
      row = {}
    end
  end
  if #row > 0 then rows[#rows + 1] = row end
  if not headers_wanted then return rows end
  local header = table.remove(rows, 1)
  local out = {}
  for i, r in ipairs(rows) do
    local rec = {}
    for j, h in ipairs(header) do rec[h] = r[j] end
    out[i] = rec
  end
  return out, header
end

-- INI ---------------------------------------------------------------------
local function ini_value(v)
  if v == "true" then return true elseif v == "false" then return false end
  local n = tonumber(v)
  if n then return n end
  local q = smatch(v, '^"(.*)"$')
  return q or v
end
local function parse_ini(text)
  local t, section = {}, nil
  local lineno = 0
  for rawline in sgsub(text, "\r\n", "\n"):gmatch("([^\n]*)\n?") do
    lineno = lineno + 1
    local line = smatch(rawline, "^%s*(.-)%s*$")
    if line == "" or smatch(line, "^[;#]") then
      -- skip
    else
      local s = smatch(line, "^%[([^%]]+)%]$")
      if s then
        section = s
        t[section] = t[section] or {}
      else
        local k, v = smatch(line, "^([%w_%.%-%[%]]+)%s*=%s*(.-)$")
        if not k then error("ini syntax error at line " .. lineno .. ": " .. line) end
        v = sgsub(v, "%s+[;#].*$", "")
        local target = section and t[section] or t
        local base = smatch(k, "^(.-)%[%]$")
        if base then
          target[base] = target[base] or {}
          table.insert(target[base], ini_value(v))
        else
          target[k] = ini_value(v)
        end
      end
    end
  end
  return t
end

-- YAML subset -------------------------------------------------------------
local function yaml_scalar(s)
  s = smatch(s, "^%s*(.-)%s*$")
  if s == "" or s == "~" or s == "null" then return nil end
  if s == "true" or s == "yes" then return true end
  if s == "false" or s == "no" then return false end
  local q = smatch(s, '^"(.*)"$') or smatch(s, "^'(.*)'$")
  if q then return q end
  local n = tonumber(s)
  if n then return n end
  local inner = smatch(s, "^%[(.*)%]$")
  if inner then
    local arr = {}
    for item in (inner .. ","):gmatch("%s*(.-)%s*,") do
      if item ~= "" then arr[#arr + 1] = yaml_scalar(item) end
    end
    return arr
  end
  return s
end

local function parse_yaml(text)
  local lines = {}
  for line in (text .. "\n"):gmatch("(.-)\n") do
    local stripped = sgsub(line, "%s+#.*$", "")
    if not smatch(stripped, "^%s*$") and not smatch(stripped, "^%s*#") and stripped ~= "---" then
      local indent = #smatch(stripped, "^( *)")
      lines[#lines + 1] = { indent = indent, text = ssub(stripped, indent + 1) }
    end
  end
  local pos = 1
  local parse_block
  parse_block = function(indent)
    local first = lines[pos]
    if not first then return nil end
    if ssub(first.text, 1, 2) == "- " or first.text == "-" then
      local list = {}
      while lines[pos] and lines[pos].indent == indent and ssub(lines[pos].text, 1, 1) == "-" do
        local rest = smatch(lines[pos].text, "^%-%s*(.*)$")
        local item_indent = indent + 2
        if rest == "" then
          pos = pos + 1
          list[#list + 1] = parse_block(lines[pos] and lines[pos].indent or item_indent)
        elseif smatch(rest, "^[%w_%-]+:%s") or smatch(rest, "^[%w_%-]+:$") then
          -- map inside list item: rewrite the line as a map entry at item indent
          lines[pos] = { indent = item_indent, text = rest }
          list[#list + 1] = parse_block(item_indent)
        else
          list[#list + 1] = yaml_scalar(rest)
          pos = pos + 1
        end
      end
      return list
    else
      local map = {}
      while lines[pos] and lines[pos].indent == indent do
        local k, v = smatch(lines[pos].text, "^([^:]+):%s*(.*)$")
        if not k then error("yaml: bad line " .. lines[pos].text) end
        pos = pos + 1
        if v == "" then
          if lines[pos] and lines[pos].indent > indent then
            map[k] = parse_block(lines[pos].indent)
          elseif lines[pos] and lines[pos].indent == indent and ssub(lines[pos].text, 1, 1) == "-" then
            map[k] = parse_block(indent)
          else
            map[k] = nil
          end
        elseif v == "|" then
          local buf = {}
          while lines[pos] and lines[pos].indent > indent do
            buf[#buf + 1] = lines[pos].text
            pos = pos + 1
          end
          map[k] = table.concat(buf, "\n")
        else
          map[k] = yaml_scalar(v)
        end
      end
      return map
    end
  end
  return parse_block(lines[1] and lines[1].indent or 0)
end

-- inputs ------------------------------------------------------------------
local function make_csv(n)
  local b = { "id,name,city,amount,note" }
  for i = 1, n do
    local note = (i % 7 == 0) and ('"has, comma and ""quotes"""') or ((i % 11 == 0) and '"multi\nline"' or "plain" .. i)
    b[#b + 1] = string.format('%d,%s,"%s",%d.%02d,%s', i, "name" .. i, (i % 3 == 0) and "New York, NY" or "Paris", i * 7, i % 100, note)
  end
  return table.concat(b, "\r\n") .. "\r\n"
end

local function make_ini(n)
  local b = { "; global settings", "debug = false", "version = 3" }
  for i = 1, n do
    b[#b + 1] = ""
    b[#b + 1] = "[server" .. i .. "]"
    b[#b + 1] = "host = host" .. i .. ".example.com ; comment"
    b[#b + 1] = "port = " .. (8000 + i)
    b[#b + 1] = "enabled = " .. tostring(i % 2 == 0)
    b[#b + 1] = 'name = "Server number ' .. i .. '"'
    b[#b + 1] = "# another comment"
    b[#b + 1] = "tags[] = a" .. i
    b[#b + 1] = "tags[] = b" .. i
    b[#b + 1] = "ratio = " .. (i / 4)
  end
  return table.concat(b, "\n")
end

local function make_yaml(n)
  local b = { "---", "app: demo", "version: 1.5", "services:" }
  for i = 1, n do
    b[#b + 1] = "  - name: svc" .. i
    b[#b + 1] = "    replicas: " .. (i % 5)
    b[#b + 1] = "    ports: [" .. (80 + i) .. ", " .. (443 + i) .. "]"
    b[#b + 1] = "    env:"
    b[#b + 1] = "      DEBUG: " .. ((i % 2 == 0) and "yes" or "no")
    b[#b + 1] = "      LEVEL: 'info' # trailing comment"
    b[#b + 1] = "    script: |"
    b[#b + 1] = "      echo start " .. i
    b[#b + 1] = "      run --fast"
    b[#b + 1] = "    deps:"
    b[#b + 1] = "      - db"
    b[#b + 1] = "      - cache" .. i
  end
  b[#b + 1] = "owner:"
  b[#b + 1] = "  name: \"Ops Team\""
  b[#b + 1] = "  email: ops@example.com"
  return table.concat(b, "\n")
end

local N = tonumber(arg and arg[1]) or 100
local csv_text, ini_text, yaml_text = make_csv(1500), make_ini(150), make_yaml(120)
local amount, ports, replicas, quoted = 0, 0, 0, 0
local last_cfg, last_yaml
for r = 1, N do
  local recs = parse_csv(csv_text, ",", true)
  for _, rec in ipairs(recs) do
    amount = amount + tonumber(rec.amount)
    if sfind(rec.note, '"', 1, true) then quoted = quoted + 1 end
  end
  local cfg = parse_ini(ini_text)
  for k, sec in pairs(cfg) do
    if type(sec) == "table" and sec.port then ports = ports + sec.port + #sec.tags end
  end
  last_cfg = cfg
  local y = parse_yaml(yaml_text)
  for _, svc in ipairs(y.services) do replicas = replicas + svc.replicas + svc.ports[2] + #svc.deps + (svc.env.DEBUG and 1 or 0) end
  last_yaml = y
end
print(string.format("amount %.2f quoted %d ports %d replicas %d", amount, quoted, ports, replicas))
print(last_cfg.server7.host, last_cfg.server7.name, last_cfg.server7.ratio, last_cfg.debug, last_cfg.version)
print(last_yaml.app, last_yaml.version, last_yaml.owner.name, last_yaml.services[3].script, last_yaml.services[3].env.LEVEL)
