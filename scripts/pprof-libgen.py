#!/usr/bin/env python3
"""Generates the library micro-benchmarks bench/lib/*.lua (H1 rewrite) plus their .expected files.

Every row runs ONE library operation per inner iteration with
  * arguments that vary with the loop index (precomputed input arrays S, SR, P7, ... indexed by k, or the running
    integer i = o*K + k), so no engine can fold or hoist the call, and
  * its result consumed into `acc` (printed with %.14g at the end), so no engine can drop it.
The loop is a nest `for o = 0, NOUT-1 do for k = 1, K do ... end end` (inner K iterations, no modulo/branch for the
input index, identical in every engine). Plain Lua 5.1-compatible source (no integer division, no bit ops, no \\u
escapes) so the very same file runs on PUC 5.4/5.5, ours and LuaJIT; sums stay below 2^53 so all three print the
same text. Rows that cannot run on LuaJIT (math.type/tointeger/ult, utf8) are listed in bench/lj/SKIP.

usage: [ONLY=row,row] [TARGET=0.08] pprof-libgen.py REF_LUA [OURS_CMD] [LUAJIT]   (run under systemd-run + timeout)
(tfront_1k.lua and tmove_1k.lua are hand-written, not generated)
  OURS_CMD (e.g. "env RUSTYNAIL_SEED=0 .../rnlua") calibrates NOUT so ours needs ~TARGET seconds (default 0.05 s,
  about 250 Mcyc); without it the reference is used. .expected = PUC output; ours and LuaJIT are compared against it.
"""
import os, re, subprocess, sys, time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
TARGET = float(os.environ.get("TARGET", "0.05"))

C = {}
SCALE = {"os_clock": 3}  # rows whose calibration run is dominated by start-up (so the loop alone would be < 100 Mcyc)


def RS(expr):
    """result stored into a table (escapes: cannot be sunk/dropped) and its length consumed"""
    return f'local r = {expr}\nring[k] = r\nacc = acc + #r'


def case(name, needs, setup, body, K=512):
    C[name] = (needs, setup, body, K)


# input arrays: name -> (statement per k, depends)
ARR = {
    "S": ('S[k] = "the quick brown fox jumps over " .. k .. " dog"', []),   # 36-38 bytes: short strings
    "SR": ("SR[k] = S[k % K + 1]", ["S"]),                                   # another (never equal) short string
    "SD": ('SD[k] = ("the quick brown fox jumps over ") .. k .. " dog"', []),  # == S[k] by value, built at run time
    "SL": ('SL[k] = "the quick brown fox jumps over the lazy dog number " .. k', []),  # > 40 bytes: long strings
    "SLR": ("SLR[k] = SL[k % K + 1]", ["SL"]),
    "SS": ('SS[k] = "k" .. k', []),
    "P7": ("P7[k] = k % 7 + 1", []),
    "P20": ("P20[k] = k % 20 + 1", []),
    "P8": ("P8[k] = k % 8 + 1", []),
    "P36": ("P36[k] = k % 36 + 1", []),
    "R": ("R[k] = 15 + k % 11", []),
    "T3": ('T3[k] = (k % 3 == 0) and "ab" or ((k % 3 == 1) and "abc" or "x")', []),
    "W": ('W[k] = WORDS[k % 6 + 1]', []),
    "NI": ("NI[k] = tostring(k * 37 + 11)", []),
    "NF": ('NF[k] = string.format("%.2f", k * 0.25 + 0.125)', []),
    "NH": ('NH[k] = string.format("%x", k * 37 + 11)', []),
    "NB": ('NB[k] = "abc" .. k', []),
    "AI": ("AI[k] = k * 7 + 3", []),
    "TT": ("TT[k] = { alpha = k, beta = 2, gamma = 3 }", []),
    "TE": ("TE[k] = {}", []),
    "TS": ("TS[k] = TSIZES[k % 8 + 1]", []),
    "TM": ("TM[k] = setmetatable({}, mt)", []),
    "V": ('V[k] = VALS[k % 3 + 1]', []),
    "ST": ("ST[k] = { 1, 2, 3, 4, 5 }", []),
}
PRE = {
    "W": 'local WORDS = { "quick", "brown", "fox", "jumps", "over", "dog" }\n',
    "TS": 'local TSIZES = {}\nfor j = 1, 8 do local t = {} for m = 1, j do t[m] = m end TSIZES[j] = t end\n',
    "TM": "local mt = {}\n",
    "V": 'local VALS = { {}, "s", 1.5 }\n',
}

S_ = "local fmt, sub, find, gsub, rep, byte, char = string.format, string.sub, string.find, string.gsub, string.rep, string.byte, string.char\n"
T_ = ("local unpack = table.unpack or unpack\n"
      "local pack = table.pack or function(...) return { n = select(\"#\", ...), ... } end\n"
      "local insert, remove, concat, sort, move = table.insert, table.remove, table.concat, table.sort, table.move\n")
M_ = "local floor, sqrt, sin, max, min, abs, random, fmod = math.floor, math.sqrt, math.sin, math.max, math.min, math.abs, math.random, math.fmod\n"
LCG = "local seed = 1\nlocal function rnd() seed = (seed * 16807) % 2147483647 return seed end\n"  # Park-Miller: exact in doubles

# ---- string.format
case("fmt_d", "", S_, RS('fmt("%d", i)'))
case("fmt_s", "S", S_, RS('fmt("%s", S[k])'))
case("fmt_f", "", S_, RS('fmt("%.3f", i * 0.37)'))
case("fmt_mixed", "S", S_, RS('fmt("%s=%d %5.2f", S[k], i, i / 7)'))
case("fmt_g", "", S_, RS('fmt("%g", i * 0.5)'))
case("fmt_x_pad", "", S_, RS('fmt("%08x|%-6d|%5s", i, i, "ab")'))
case("fmt_q", "S", S_, RS('fmt("%q", S[k])'))
# ---- string search / sub
case("sub", "S P7", S_, RS('sub(S[k], P7[k], P7[k] + 10)'))
case("sub_1char", "S P36", S_, RS('sub(S[k], P36[k], P36[k])'))
case("find_plain", "S W", S_, 'acc = acc + find(S[k], W[k], 1, true)')
case("find_pat", "S", S_, 'acc = acc + find(S[k], "j%a+s")')
case("find_cap", "S", S_, 'local a, b, c = find(S[k], "(%a+) (%a+)")\nacc = acc + b')
case("match", "S", S_, RS('S[k]:match("(%d+) dog")'))
case("gmatch_words", "S", S_, 'for w in S[k]:gmatch("%a+") do acc = acc + 1 end')
case("gsub_str", "S", S_, RS('gsub(S[k], "o", "0")'))
case("gsub_fn", "S", S_, RS('gsub(S[k], "%a+", function(w) return w end)'))
case("gsub_tbl", "S", S_ + 'local t = { fox = "cat", dog = "cow" }\n', RS('gsub(S[k], "%a+", t)'))
# ---- string construction
case("rep", "T3 R", S_, RS('rep(T3[k], R[k])'))
case("rep_sep", "T3 R", S_, RS('rep(T3[k], R[k], ",")'))
case("byte", "S P36", S_, 'acc = acc + byte(S[k], P36[k])')
case("byte_multi", "S P7", S_, 'local a, b, c, d = byte(S[k], P7[k], P7[k] + 3)\nacc = acc + d + a')
case("char", "", S_, RS('char(65 + k % 26, 66, 67)'))
case("upper", "S", S_, RS('S[k]:upper()'))
case("lower", "S", S_, RS('S[k]:lower()'))
case("reverse", "S", S_, RS('S[k]:reverse()'))
case("len_op", "S", S_, 'local s = S[k]\nacc = acc + #s + s:len()')
case("concat_op", "", S_, RS('"a" .. i .. "b"'))
case("concat_op_str", "S SR", S_, RS('S[k] .. SR[k]'))
case("str_eq", "S SR", S_, 'if S[k] == SR[k] then acc = acc + 1 end')
case("str_eq_dyn", "S SD", S_, 'if S[k] == SD[k] then acc = acc + 1 end')
case("str_eq_long", "SL SLR", S_, 'if SL[k] == SLR[k] then acc = acc + 1 end')
case("str_lt", "S SR", S_, 'if SR[k] < S[k] then acc = acc + 1 end')
case("str_lt_short", "SS", S_, 'if SS[k] < SS[k % K + 1] then acc = acc + 1 end')
case("tostring_int", "", S_, RS('tostring(i)'))
case("tostring_flt", "", S_, RS('tostring(i + 0.5)'))
case("tostring_tbl", "TE", S_, 'if #tostring(TE[k]) > 7 then acc = acc + 1 end')
case("tonumber_int", "NI", S_, 'acc = acc + tonumber(NI[k])')
case("tonumber_flt", "NF", S_, 'acc = acc + tonumber(NF[k])')
case("tonumber_base", "NH", S_, 'acc = acc + tonumber(NH[k], 16)')
case("tonumber_num", "", S_, 'acc = acc + tonumber(i)')
case("tonumber_bad", "NB", S_, 'if not tonumber(NB[k]) then acc = acc + 1 end')
# ---- table library (heavier iterations: K = 64 or 16)
case("tinsert_append", "", T_, 'local t = {}\nfor j = 1, 50 do insert(t, j + k) end\nacc = acc + #t + t[50]', 64)
case("tinsert_idx", "", T_, 'local t = {}\nfor j = 1, 50 do t[#t + 1] = j + k end\nacc = acc + #t + t[50]', 64)
case("tinsert_front", "", T_, 'local t = {1, 2, 3, 4, 5, 6, 7, 8}\ninsert(t, 1, i)\nacc = acc + #t + t[1]')
case("tremove_pop", "", T_ + 'local base = {}\nfor j = 1, 50 do base[j] = j end\n',
     'local t = move(base, 1, 50, 1, {})\nt[50] = k\nlocal v = 0\nfor j = 1, 50 do v = v + remove(t) end\nacc = acc + v + #t', 64)
case("tremove_front", "", T_, 'local t = {k, 2, 3, 4, 5, 6, 7, 8}\nacc = acc + remove(t, 1) + #t')
case("tconcat", "S P20", T_ + 'local t = {}\nfor j = 1, 20 do t[j] = "item" .. j end\n', 't[P20[k]] = S[k]\n' + RS('concat(t, ",")'), 64)
case("tconcat_num", "P20", T_ + 'local t = {}\nfor j = 1, 20 do t[j] = j end\n', 't[P20[k]] = k\n' + RS('concat(t, " ")'), 64)
case("tsort_int", "", T_ + LCG, 'local t = {}\nfor j = 1, 100 do t[j] = rnd() end\nsort(t)\nacc = acc + t[1] % 3 + t[100] % 5', 16)
case("tsort_cmp", "", T_ + LCG + 'local function cmp(a, b) return a > b end\n', 'local t = {}\nfor j = 1, 100 do t[j] = rnd() end\nsort(t, cmp)\nacc = acc + t[1] % 3 + t[100] % 5', 16)
case("tsort_str", "", T_ + LCG + 'local pool = {}\nfor j = 1, 100 do pool[j] = "k" .. rnd() end\n',
     'local t = move(pool, 1, 100, 1, {})\nt[k % 100 + 1] = "k" .. i\nsort(t)\nacc = acc + #t[1] + #t[100]', 16)
case("tunpack", "", T_, 'local t = {1, 2, k, 4, 5}\nlocal a, b, c, d, e = unpack(t)\nacc = acc + e + c')
case("tpack", "", T_, 'local t = pack(i, 2, 3)\nring[k] = t\nacc = acc + t.n + t[1]')
case("tmove", "", T_ + 'local t = {1, 2, 3, 4, 5, 6, 7, 8}\nlocal u = {}\n', 't[1] = k\nmove(t, 1, 8, 1, u)\nacc = acc + #u + u[1]')
case("len_tbl", "TS", T_, 'acc = acc + #TS[k]')
# ---- math
case("m_floor", "", M_, 'acc = acc + floor(i * 0.37)')
case("m_sqrt", "", M_, 'acc = acc + sqrt(i)')
case("m_sin", "", M_, 'acc = acc + sin(i)')
case("m_max", "", M_, 'acc = acc + max(i, 100, 7)')
case("m_min", "", M_, 'acc = acc + min(i, 3)')
case("m_abs", "", M_, 'acc = acc + abs(-i)')
# random numbers differ between engines: consume through a range check only
case("m_random", "", M_ + 'math.randomseed(42)\n', 'local r = random(1, 100)\nif r >= 1 and r <= 100 then acc = acc + 1 end')
case("m_random_f", "", M_ + 'math.randomseed(42)\n', 'local r = random()\nif r >= 0 and r < 1 then acc = acc + 1 end')
case("m_fmod", "", M_, 'acc = acc + fmod(i, 7)')
case("m_tointeger", "", M_ + 'local tointeger = math.tointeger\n', 'acc = acc + tointeger(i + 0.0)')
case("m_type", "", M_, 'if math.type(i) == "integer" then acc = acc + 1 end')
case("m_ult", "", M_, 'if math.ult(i, 5) then acc = acc + 1 end')
case("m_pow_op", "", M_, 'acc = acc + i ^ 2')
# ---- varargs, iteration
case("sel_count", "", 'local select = select\nlocal function f(...) return select("#", ...) end\n', 'acc = acc + f(k, 2, 3, 4)')
case("sel_n", "", 'local select = select\nlocal function f(...) return (select(3, ...)) end\n', 'acc = acc + f(1, 2, k, 4)')
case("sel_loop", "", 'local select = select\nlocal function f(...) local s = 0 for j = 1, select("#", ...) do s = s + (select(j, ...)) end return s end\n', 'acc = acc + f(k, 2, 3, 4)', 128)
case("it_pairs", "", 'local t = {}\nfor j = 1, 50 do t[j] = j end\n', 't[1] = k\nfor _, v in pairs(t) do acc = acc + v end', 32)
case("it_pairs_hash", "", 'local t = {}\nfor j = 1, 50 do t["k" .. j] = j end\n', 't.k1 = k\nfor _, v in pairs(t) do acc = acc + v end', 32)
case("it_ipairs", "", 'local t = {}\nfor j = 1, 50 do t[j] = j end\n', 't[1] = k\nfor _, v in ipairs(t) do acc = acc + v end', 32)
case("it_next", "", 'local t = {}\nfor j = 1, 50 do t[j] = j end\nlocal next = next\n', 't[1] = k\nlocal key, v = next(t)\nwhile key do acc = acc + v key, v = next(t, key) end', 32)
case("it_next_empty", "TE", 'local next = next\n', 'if next(TE[k]) == nil then acc = acc + 1 end')
# ---- misc
case("type_call", "V", 'local type = type\n', 'if type(V[k]) == "table" then acc = acc + 1 end')
case("rawget_set", "P8", 'local t = {}\nlocal rawget, rawset = rawget, rawset\n', 'rawset(t, P8[k], i)\nacc = acc + rawget(t, P8[k])')
case("setmt", "P20", 'local mt = {}\nlocal setmetatable = setmetatable\nlocal ring = {}\n', 'local t = setmetatable({ x = k }, mt)\nring[P20[k]] = t\nacc = acc + t.x')
case("getmt", "TM", 'local getmetatable = getmetatable\n', 'if getmetatable(TM[k]) == mt then acc = acc + 1 end')
case("pcall_ok", "", 'local pcall = pcall\nlocal function f(x) return x * 2 + 1 end\n', 'local ok, v = pcall(f, i)\nacc = acc + v')
case("pcall_err", "", 'local pcall = pcall\nlocal function f(x) error("x") end\n', 'local ok = pcall(f, i)\nif not ok then acc = acc + 1 end', 128)
case("error_tbl", "", 'local pcall, error = pcall, error\nlocal e = {}\nlocal function f(x) error(e) end\n', 'local ok = pcall(f, i)\nif not ok then acc = acc + 1 end', 128)
case("assert_ok", "AI", 'local assert = assert\n', 'local v = assert(AI[k], "msg")\nacc = acc + v')
case("os_clock", "", 'local clock = os.clock\n', 'acc = acc + (clock() > 0 and 1 or 0)')
case("co_wrap", "", 'local cw = coroutine.wrap\n', 'local f = cw(function(a) local b = coroutine.yield(a) return b end)\nacc = acc + f(k) + f(2)', 128)
case("co_yield", "", 'local co = coroutine.wrap(function() local n = 0 while true do n = n + 1 coroutine.yield(n) end end)\n', 'acc = acc + co()')
case("utf8_len", "NB", 'local u8 = utf8\n', 'acc = acc + u8.len(NB[k])')
case("str_method_len", "S", S_, 'acc = acc + S[k]:len()')
case("load_small", "", 'local load = load\n', 'local f = load("return " .. k .. " + 1")\nacc = acc + f()', 128)
case("string_key_dyn", "", 'local t = {}\nfor j = 1, 20 do t["key" .. j] = j end\nlocal keys = {}\nfor j = 1, 20 do keys[j] = "key" .. j end\n',
     'acc = acc + t[keys[k % 20 + 1]]')
case("string_key_const", "TT", "", 'local t = TT[k]\nacc = acc + t.alpha + t.gamma')
case("str_idx_concat_key", "P20", 'local t = {}\nfor j = 1, 20 do t["key" .. j] = j end\n', 'acc = acc + t["key" .. P20[k]]')


def build_arrays(needs):
    order, seen = [], set()

    def add(n):
        if n in seen:
            return
        seen.add(n)
        for d in ARR[n][1]:
            add(d)
        order.append(n)
    for n in needs.split():
        add(n)
    out = ""
    for n in order:
        out += PRE.get(n, "")
    for n in order:
        out += f"local {n} = {{}}\nfor k = 1, K do {ARR[n][0]} end\n"
    return out


def src(name, nout):
    needs, setup, body, K = C[name]
    arrs = build_arrays(needs)
    uses_i = re.search(r"\bi\b", body) is not None
    head = f"-- generated by scripts/pprof-libgen.py (H1: varying arguments, result consumed into acc)\nlocal K = {K}\n"
    if "ring[" in body:
        head += "local ring = {}\n"
    loop = f"local acc = 0\nfor o = 0, {nout - 1} do\n"
    if uses_i:
        loop += "local base = o * K\n"
    loop += "for k = 1, K do\n"
    if uses_i:
        loop += "local i = base + k\n"
    loop += body + "\nend\nend\nprint(string.format(\"%.14g\", acc))\n"
    return head + setup + arrs + loop


def run(cmd, path, timeout=120):
    t0 = time.perf_counter()
    r = subprocess.run(cmd + [path], stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)
    return r, time.perf_counter() - t0


def main():
    ref = sys.argv[1].split()
    ours = sys.argv[2].split() if len(sys.argv) > 2 and sys.argv[2] else ref
    lj = sys.argv[3].split() if len(sys.argv) > 3 else None
    out = os.path.join(ROOT, "bench/lib")
    skip = set()
    for line in open(os.path.join(ROOT, "bench/lj/SKIP")):
        line = line.split("#")[0].strip()
        if line.startswith("lib/"):
            skip.add(line[4:])
    os.makedirs(out, exist_ok=True)
    only = [x for x in os.environ.get("ONLY", "").split(",") if x]
    for name in C:
        if only and name not in only:
            continue
        p = os.path.join(out, name + ".lua")
        n = 100
        dt = 0
        for _ in range(4):
            open(p, "w").write(src(name, n))
            r, dt = run(ours, p)
            if r.returncode != 0:
                print("FAIL", name, r.stderr.decode()[:300])
                break
            n = max(1, int(n * TARGET / max(dt - 0.004, 0.001)))
        n = n * SCALE.get(name, 1)
        if re.search(r"\bi\b", C[name][2]):
            n = min(n, 120000000 // C[name][3])  # keep integer sums of i below 2^53 (exact in doubles too)
        open(p, "w").write(src(name, n))
        r, _ = run(ref, p)
        if r.returncode != 0:
            print("FAIL(ref)", name, r.stderr.decode()[:300])
            continue
        exp = r.stdout
        open(p[:-4] + ".expected", "wb").write(exp)
        msg = ""
        ro, dto = run(ours, p)
        if ro.stdout != exp:
            msg += f" OURS-DIFF({ro.stdout!r})"
        if lj and name not in skip:
            rl, dtl = run(lj, p)
            if rl.stdout != exp:
                msg += f" LJ-DIFF({rl.stdout!r} vs {exp!r} {rl.stderr.decode()[:100]})"
            msg += f" lj {dtl:.3f}s"
        print(f"{name:20} nout={n:8d} K={C[name][3]:4d} ours {dto:.3f}s exp={exp.decode().strip()}{msg}")


main()
