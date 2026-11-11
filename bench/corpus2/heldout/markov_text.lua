-- markov_text: imitates a text toy: order-2 word Markov chain over a generated corpus, text
-- generation with an LCG, character n-gram statistics and a similarity score. small: N=1
local N = tonumber(arg and arg[1]) or 190

local seed = 31337
local function rnd(n)
  seed = (seed * 16807) % 2147483647
  return seed % n + 1
end

-- ------------------------------------------------------------ corpus generation
local ONSETS = { "b", "br", "c", "d", "f", "g", "gl", "h", "k", "l", "m", "n", "p", "pr", "r", "s", "st", "t", "v", "w" }
local VOWELS = { "a", "e", "i", "o", "u", "ai", "ou" }
local CODAS = { "", "n", "r", "s", "t", "l", "nd", "st" }

local vocab = {}
for i = 1, 90 do
  local w = ONSETS[rnd(#ONSETS)] .. VOWELS[rnd(#VOWELS)]
  if rnd(2) == 1 then w = w .. ONSETS[rnd(#ONSETS)] .. VOWELS[rnd(#VOWELS)] end
  vocab[i] = w .. CODAS[rnd(#CODAS)]
end
local function noun() return vocab[rnd(30)] end
local function verb() return vocab[30 + rnd(30)] end
local function adj() return vocab[60 + rnd(30)] end

local SENTENCE_FORMS = {
  function() return { "the", adj(), noun(), verb(), "the", noun() } end,
  function() return { "a", noun(), "and", "a", noun(), verb(), "slowly" } end,
  function() return { "every", noun(), verb(), "when", "the", adj(), noun(), verb() } end,
  function() return { "the", noun(), "of", "the", noun(), verb(), adj(), noun() } end,
  function() return { "then", "the", noun(), verb(), "and", verb() } end,
}

local function make_corpus(nsent)
  local words = {}
  for _ = 1, nsent do
    local s = SENTENCE_FORMS[rnd(#SENTENCE_FORMS)]()
    for _, w in ipairs(s) do words[#words + 1] = w end
    words[#words + 1] = "."
  end
  return words
end

-- ------------------------------------------------------------ chain construction
local function build_chain(words)
  local chain, nkeys = {}, 0
  for i = 1, #words - 2 do
    local key = words[i] .. " " .. words[i + 1]
    local list = chain[key]
    if not list then
      list = {}
      chain[key] = list
      nkeys = nkeys + 1
    end
    list[#list + 1] = words[i + 2]
  end
  return chain, nkeys
end

local function generate(chain, words, count)
  local i = rnd(#words - 2)
  local w1, w2 = words[i], words[i + 1]
  local out = { w1, w2 }
  for _ = 1, count - 2 do
    local list = chain[w1 .. " " .. w2]
    if not list then
      i = rnd(#words - 2)
      w1, w2 = words[i], words[i + 1]
      list = chain[w1 .. " " .. w2]
    end
    local nxt = list[rnd(#list)]
    out[#out + 1] = nxt
    w1, w2 = w2, nxt
  end
  return out
end

local function render(words)
  local text = table.concat(words, " ")
  text = text:gsub(" %.", ".")
  return text
end

-- ------------------------------------------------------------ statistics
local function ngram_counts(text, n)
  local counts = {}
  for i = 1, #text - n + 1 do
    local g = text:sub(i, i + n - 1)
    counts[g] = (counts[g] or 0) + 1
  end
  return counts
end

local function top_entries(counts, limit)
  local keys = {}
  for k in pairs(counts) do keys[#keys + 1] = k end
  table.sort(keys, function(a, b)
    if counts[a] ~= counts[b] then return counts[a] > counts[b] end
    return a < b
  end)
  local out = {}
  for i = 1, math.min(limit, #keys) do out[i] = { keys[i], counts[keys[i]] } end
  return out, #keys
end

local function cosine(a, b)
  local dot, na, nb = 0, 0, 0
  for k, v in pairs(a) do
    na = na + v * v
    local o = b[k]
    if o then dot = dot + v * o end
  end
  for _, v in pairs(b) do nb = nb + v * v end
  if na == 0 or nb == 0 then return 0 end
  return dot / (math.sqrt(na) * math.sqrt(nb))
end

local function jaccard(wa, wb)
  local sa, sb, inter, union = {}, {}, 0, 0
  for _, w in ipairs(wa) do sa[w] = true end
  for _, w in ipairs(wb) do sb[w] = true end
  for w in pairs(sa) do
    union = union + 1
    if sb[w] then inter = inter + 1 end
  end
  for w in pairs(sb) do
    if not sa[w] then union = union + 1 end
  end
  return inter / union
end

local function word_freq(words)
  local f = {}
  for _, w in ipairs(words) do f[w] = (f[w] or 0) + 1 end
  return f
end

-- ------------------------------------------------------------ main
local corpus = make_corpus(N * 300)
print("corpus words:", #corpus, "vocabulary sample: " .. table.concat(vocab, " ", 1, 8))

local chain, nkeys = build_chain(corpus)
local branching, maxb, maxkey = 0, 0, ""
for k, list in pairs(chain) do
  branching = branching + #list
  if #list > maxb or (#list == maxb and k < maxkey) then maxb, maxkey = #list, k end
end
print(string.format("chain keys=%d transitions=%d avg branching=%.3f", nkeys, branching, branching / nkeys))
print(string.format("most common context: '%s' (%d followers)", maxkey, maxb))

local gen = generate(chain, corpus, N * 400)
local text = render(gen)
print("generated words:", #gen)
print("sample: " .. text:sub(1, 150))

-- how much of the generated text exists verbatim in the corpus (as 3-grams)?
local corpus_tri = {}
for i = 1, #corpus - 2 do corpus_tri[corpus[i] .. "|" .. corpus[i + 1] .. "|" .. corpus[i + 2]] = true end
local novel = 0
for i = 1, #gen - 2 do
  if not corpus_tri[gen[i] .. "|" .. gen[i + 1] .. "|" .. gen[i + 2]] then novel = novel + 1 end
end
print("novel word trigrams in output:", novel)

local ctext = render(corpus)
for _, n in ipairs({ 1, 2, 3 }) do
  local counts = ngram_counts(ctext, n)
  local top, distinct = top_entries(counts, 5)
  local parts = {}
  for _, e in ipairs(top) do parts[#parts + 1] = string.format("%q:%d", e[1], e[2]) end
  print(string.format("%d-grams distinct=%d top: %s", n, distinct, table.concat(parts, " ")))
end

-- similarity: generated text vs corpus, vs a second independent corpus, vs itself
local other = make_corpus(N * 300)
local other_text = render(other)
local c3, g3, o3 = ngram_counts(ctext, 3), ngram_counts(text, 3), ngram_counts(other_text, 3)
print(string.format("cosine(gen, corpus)   = %.4f", cosine(g3, c3)))
print(string.format("cosine(other, corpus) = %.4f", cosine(o3, c3)))
print(string.format("cosine(corpus, corpus)= %.4f", cosine(c3, c3)))
print(string.format("jaccard words gen/corpus = %.4f", jaccard(gen, corpus)))
print(string.format("jaccard words gen/other  = %.4f", jaccard(gen, other)))

local freq = word_freq(gen)
local ranked, top = top_entries(freq, 6)
local parts = {}
for _, e in ipairs(ranked) do parts[#parts + 1] = e[1] .. "=" .. e[2] end
print("top generated words: " .. table.concat(parts, " "))

local check = 0
for i = 1, #text do check = (check * 131 + text:byte(i)) % 1000000007 end
print("text checksum:", check, #text)
