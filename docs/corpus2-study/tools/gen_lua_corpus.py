#!/usr/bin/env python3
"""Generate a deterministic corpus of small Lua chunks into tests/lua/gen/ (Lua 5.5) and
tests/lua/gen54/ (Lua 5.4: same chunks minus the 5.5-only ones, plus 5.4-specific ones).

Used by crates/rn-compiler/tests/bytecode_diff.rs (our bytecode must be identical to the reference
`luac`). Everything is seeded: re-running produces byte-identical files.

    python3 tools/gen_lua_corpus.py            # writes tests/lua/gen/*.lua and tests/lua/gen54/*.lua
"""
import os
import random
import sys

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "lua", "gen")
OUT54 = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "tests", "lua", "gen54")
rng = random.Random(0x5EED_1234)
files = {}
only55 = set()  # chunks that use 5.5-only syntax (not generated for 5.4)
only54 = set()  # 5.4-specific chunks (not generated for 5.5)


def add(name, text, only=None):
    files[name + ".lua"] = text if text.endswith("\n") else text + "\n"
    if only == "55":
        only55.add(name + ".lua")
    elif only == "54":
        only54.add(name + ".lua")


INTS = ["0", "1", "2", "-1", "7", "127", "128", "-127", "-128", "255", "256", "65535", "65536",
        "131071", "131072", "-65535", "-65536", "0x7fffffff", "0x7fffffffffffffff",
        "math.maxinteger", "math.mininteger", "9223372036854775807", "9223372036854775808",
        "0xffffffffffffffff", "1000000007", "-9223372036854775807"]
FLTS = ["0.0", "-0.0", "1.0", "-1.0", "0.5", "2.5", "1e100", "-1e100", "1e308", "1e-320", "3.14159",
        "127.0", "128.0", "-127.0", "-128.0", "255.0", "65535.0", "2^53", "2^63", "1e15", "0x1p4",
        "0x.8", "1/0", "-1/0", "0/0", "123456789012345678.0", "5e-324", "0.1", "100.0"]
STRS = ['"a"', '"hello"', '""', "'x'", '"\\n"', '"a\\0b"', '"\\u{48}\\u{49}"', '"\\x41\\65\\z  B"',
        '"%d"', '"a long string constant that goes beyond forty characters in length"',
        '[[long\nbracket]]', '"tab\\there"', '"quote\\"d"']
BINOPS = ["+", "-", "*", "/", "//", "%", "^", "&", "|", "~", "<<", ">>", "..", "==", "~=", "<", "<=",
          ">", ">=", "and", "or"]
NUMOPS = ["+", "-", "*", "/", "//", "%", "^", "&", "|", "~", "<<", ">>"]
UNOPS = ["-", "~", "not ", "#"]
VARS = ["a", "b", "c", "d"]


def atom(env):
    r = rng.random()
    if r < 0.30:
        return rng.choice(INTS)
    if r < 0.45:
        return rng.choice(FLTS)
    if r < 0.55:
        return rng.choice(STRS)
    if r < 0.62:
        return rng.choice(["nil", "true", "false"])
    if r < 0.85:
        return rng.choice(env)
    if r < 0.92:
        return "t.%s" % rng.choice(["x", "y", "z"])
    if r < 0.96:
        return "t[%s]" % rng.choice(["1", "2", "k", "a"])
    return "f(%s)" % rng.choice(env)


def expr(env, depth):
    if depth <= 0 or rng.random() < 0.25:
        return atom(env)
    r = rng.random()
    if r < 0.15:
        u = rng.choice(UNOPS)
        return "%s%s" % (u, paren(expr(env, depth - 1)))
    if r < 0.2:
        return "(%s)" % expr(env, depth - 1)
    op = rng.choice(BINOPS)
    return "%s %s %s" % (paren(expr(env, depth - 1)), op, paren(expr(env, depth - 1)))


def paren(e):
    return "(%s)" % e if (" " in e and not e.startswith("(")) else e


def numexpr(depth):
    if depth <= 0 or rng.random() < 0.3:
        return rng.choice(INTS + FLTS[:20])
    if rng.random() < 0.15:
        return "%s(%s)" % (rng.choice(["-", "~"]), numexpr(depth - 1))
    return "(%s) %s (%s)" % (numexpr(depth - 1), rng.choice(NUMOPS), numexpr(depth - 1))


# --- 1. constant folding edge cases (pure numeric expressions) ---------------------------------
for i in range(40):
    lines = ["local r = {}"]
    for j in range(6):
        lines.append("r[%d] = %s" % (j + 1, numexpr(3)))
    lines.append("return r")
    add("fold_%03d" % i, "\n".join(lines))

# fixed folding edge cases
add("fold_edge", """
local a = {
  1//0 == nil, 0.0/0.0, -(0.0), 0.0 * -1, 1e308 * 10, -1e308 * 10,
  math.maxinteger + 1, math.mininteger - 1, math.maxinteger * 2,
  1 << 63, 1 << 64, 1 >> 1, -1 >> 1, -1 << 1, 3 & 5.0, 3 | 4.5, ~0, ~5.0,
  7 // 2, -7 // 2, 7 % -3, -7 % 3, 7.5 // 2, 7.5 % 2, 2^-1, 8 // 0.0, -8 // 0.0,
  0x10 + 0xA, 0xff // 3, "10" + 1, 10 .. 20, 2^63 == math.mininteger,
  1e15 + 0.5, 3 % math.huge, -3 % math.huge, 3 % -math.huge,
}
return a
""")

# --- 2. random expressions with locals / upvalues / tables / calls -------------------------------
for i in range(60):
    env = VARS[:]
    lines = ["local a, b, c, d = ...", "local t = {x = 1, y = 2, z = 3, 10, 20}", "local k = 1",
             "local function f(x) return x end"]
    for j in range(5):
        lines.append("local v%d = %s" % (j, expr(env, 3)))
        env.append("v%d" % j)
    lines.append("return %s, %s" % (expr(env, 3), expr(env, 2)))
    add("expr_%03d" % i, "\n".join(lines))

# --- 3. conditions / short circuit ------------------------------------------------------------------
for i in range(30):
    env = VARS[:]
    cond = expr(env, 3)
    cond2 = expr(env, 2)
    body = "\n".join(
        [
            "local a, b, c, d = ...",
            "local t = {x = 1}",
            "local function f(x) return x end",
            "local k = 1",
            "if %s then a = 1 elseif %s then a = 2 else a = 3 end" % (cond, cond2),
            "while %s do a = a + 1; if %s then break end end" % (expr(env, 2), expr(env, 2)),
            "repeat local q = %s until %s" % (expr(env, 2), expr(env, 2) + " or q"),
            "local r = %s" % expr(env, 3),
            "return not (%s), a, r" % expr(env, 2),
        ]
    )
    add("cond_%03d" % i, body)

# --- 4. comparison chains and operand forms -------------------------------------------------------------
cmp_lines = ["local a, b = ...", "local r = {}"]
vals = ["0", "1", "-1", "127", "128", "-128", "255", "256", "1.0", "127.0", "128.0", "0.5", "1e100",
        "'s'", "nil", "true", "a", "b", "math.huge"]
for op in ["<", "<=", ">", ">=", "==", "~="]:
    for _ in range(6):
        cmp_lines.append("r[#r+1] = %s %s %s" % (rng.choice(vals), op, rng.choice(vals)))
    cmp_lines.append("if %s %s %s then r[#r+1] = 1 end" % (rng.choice(vals), op, rng.choice(vals)))
cmp_lines.append("return r")
add("cmp_forms", "\n".join(cmp_lines))
add("cmp_chain", """
local a, b, c = ...
local x = a < b == (b < c)
local y = a == b ~= (b == c)
local z = not a == not b
local w = a < 1 and b > 2 or c <= 3 and not (a >= 4)
return x, y, z, w
""")

# --- 5. operator precedence ---------------------------------------------------------------------------------
prec = [
    "a + b * c ^ d ^ 2", "-a ^ 2", "-a ^ -b", "not a == b", "a .. b .. c .. d", "a .. b + c .. d",
    "a or b and c", "a and b or c and d", "a < b == c", "#a + #b * 2", "-#a", "~a & b | c ~ d",
    "a << 1 + 2", "a & b == c", "a | b ~ c & d << 1 >> 2", "a // b % c * d / a", "2 ^ -3 ^ 2",
    "- - a", "not not a", "~ ~ a", "a .. - b", "(a + b) * (c - d)", "a % b // c", "a ^ b % c",
    "a - b - c - d", "a / b / c", "a ^ b ^ c", "a .. b == c .. d", "a + 1 .. b + 2",
    "#t.x + #t[1]", "-t.x ^ t[2]", "f(a) .. f(b) .. f(c)", "a and f(b) or f(c)",
]
for i, p in enumerate(prec):
    add("prec_%02d" % i, "local a, b, c, d = ...\nlocal t = {x = 'x', 'y', 'z'}\nlocal function f(x) return x end\nreturn %s" % p)

# --- 6. string escapes and constants ------------------------------------------------------------------------------
add("strings", "\n".join("local s%d = %s" % (i, s) for i, s in enumerate(STRS)) + "\nreturn s0")
add("escapes", r'''
return "\a\b\f\n\r\t\v\\\"\'", '\65\066\x41\u{20AC}\u{10FFFF}\u{7FFFFFFF}', "a\z
       b", "line1\
line2", [==[
x]]y]==], "\0\00\000\1234"
''')
add("long_strings", 'return [[' + "x" * 50 + ']], "' + "y" * 41 + '", "' + "z" * 40 + '"')

# --- 7. upvalue chains --------------------------------------------------------------------------------------------------
for depth in (2, 5, 10, 30):
    lines = ["local u0 = 0"]
    ind = ""
    for d in range(depth):
        lines.append("%slocal function f%d(p%d)" % (ind, d, d))
        ind += " "
        lines.append("%slocal u%d = p%d + u%d" % (ind, d + 1, d, d))
    lines.append("%sreturn u0 + u%d" % (ind, depth))
    for d in range(depth):
        ind = ind[:-1]
        lines.append("%send" % ind)
    lines.append("return f0")
    add("upchain_%02d" % depth, "\n".join(lines))
add("upvalue_loops", """
local fs = {}
for i = 1, 3 do
  local x = i
  fs[#fs+1] = function() x = x + 1; return x + i end
end
local j = 0
while j < 3 do
  local y = j
  fs[#fs+1] = function() return y end
  j = j + 1
end
repeat
  local z = j
  fs[#fs+1] = function() return z end
until z > 2
for k, v in pairs(fs) do local w = k; fs[k] = function() return w, v end end
return fs
""")
add("closures_shared", """
local a, b = 1, 2
local function f() a = a + 1; return function() b = b + a; return a, b end end
local g = f()
return g, f, function() return a, b, _ENV end
""")

# --- 8. numeric for variants ----------------------------------------------------------------------------------------------------
fors = ["for i = 1, 10 do end", "for i = 10, 1, -1 do end", "for i = 1.0, 2.0, 0.5 do end",
        "for i = 1, 0 do end", "for i = a, b do end", "for i = a, b, c do end", "for i = 1, #t do t[i] = i end",
        "for i = math.maxinteger - 1, math.maxinteger do end", "for i = 1, 3, 0 do end",
        "for i = 1, 2 do for j = i, 2 do local x = i + j end end", "for i = 1, 10 do if i > 5 then break end end",
        "for i = 1, 3 do local f = function() return i end end", "for i = 'a', 2 do end",
        "for i = -1, -10, -3 do local k <const> = i end", "for _ = 1, 3 do goto cont; ::cont:: end"]
for i, f in enumerate(fors):
    add("for_num_%02d" % i, "local a, b, c = ...\nlocal t = {}\n" + f)
gen_fors = ["for k, v in pairs(t) do end", "for k, v in next, t do end", "for k in pairs(t) do local x = k end",
            "for a, b, c, d in f() do end", "for i, v in ipairs(t) do if v then break end end",
            "for k, v in pairs(t) do for k2 in pairs(v) do end end", "for k, v in f(t) do local g = function() return k, v end end",
            "for k, v in pairs(t), nil, nil, io.stdout do end", "for k, v in (f()) do end",
            "for l in io.lines() do goto continue; ::continue:: end"]
for i, f in enumerate(gen_fors):
    add("for_gen_%02d" % i, "local t, f = ...\n" + f)

# --- 9. goto / labels --------------------------------------------------------------------------------------------------------------------------
gotos = [
    "goto l1; do local x end ::l1::",
    "::top:: local i = 1; if i then goto top end",
    "do goto done end ::done::",
    "for i = 1, 3 do for j = 1, 3 do if j == 2 then goto continue end end ::continue:: end",
    "do local a; goto e; local b; ::e:: end",
    "::a:: ::b:: ::c:: goto a",
    "while true do if x then goto out end end ::out::",
    "repeat local k = 1; if k then goto skip end; local z = 2; ::skip:: until true",
    "do ::l:: end do ::l:: end",
    "local f = function() goto x; ::x:: end",
    "do local u; local g = function() return u end; goto fin; ::fin:: end",
    "for i = 1, 2 do local u; local g = function() return u end; if i then goto nxt end; u = 1; ::nxt:: end",
    "goto f1; ::f2:: goto f3; ::f1:: goto f2; ::f3::",
    "while 1 do local x <close> = nil; goto out end ::out::",
    "do goto l2; local a ::l2:: ; end",
]
for i, g in enumerate(gotos):
    add("goto_%02d" % i, "local x = ...\n" + g)
add("break_forms", """
local t = {...}
while true do break end
repeat if t[1] then break end until false
for i = 1, 3 do for j = 1, 3 do if i == j then break end end if i then break end end
for k in pairs(t) do local f = function() return k end; if f then break end end
return t
""")

# --- 10. method chains / calls ----------------------------------------------------------------------------------------------------------------------
add("methods", """
local o = ...
o:a():b(1):c(1, 2):d "str" :e {1, 2} :f(...)
local x = o.a.b.c:d(o.e[1], o["f"]):g()
o.a.b:c(function(...) return ... end)
a.b.c.d = o:m()
return o:x(), o:y()
""")
add("call_forms", """
local f, a, b = ...
f()
f(a)
f(a, b)
f(a, f(b))
f(f(f()))
f{1, 2}
f"s"
f[[s]]
f(...)
f(a, ...)
f((...))
return f(a), (f(b)), f(...), ...
""")
for n in (1, 2, 3, 10, 40):
    add("many_args_%02d" % n, "local f = ...\nreturn f(%s)" % ", ".join(str(i) for i in range(n)))

# --- 11. varargs in every position ----------------------------------------------------------------------------------------------------------------
va = ["return ...", "return (...)", "return ..., 1", "return 1, ...", "local a = ...", "local a, b, c = ...",
      "local a, b = 1, ...", "local a, b = ..., 1", "local t = {...}", "local t = {..., 1}", "local t = {1, ...}",
      "local t = {(...)}", "local t = {..., ...}", "print(...)", "print(..., 1)", "print(1, ...)", "a, b = ...",
      "a, b = 1, ...", "return select('#', ...), ...", "local function f(...) return ... end return f(...)",
      "local function f(a, ...) return a, ... end return f(...)", "return function(...) return ... end",
      "local t = {n = select('#', ...), ...}", "return {...}, {..., n = 1}",
      "return #{...}, ... == nil"]
for i, v in enumerate(va):
    add("vararg_%02d" % i, v)
named = ["local function f(...t) return t end", "local function f(a, ...t) return t[1], #t, a end",
         "local function f(...t) t.x = 1; return t end", "local function f(...t) return ... end",
         "local function f(...t) return function() return t end end",
         "local function f(...t) t[1] = 2; return ... end", "local function f(...t) return t.n end",
         "local function f(...t) local u = t; return u[1] end", "function g(...args) return args end",
         "local function f(...t) return t[1], t[2], t[3] end", "local function f(...t) for i = 1, #t do end end"]
for i, v in enumerate(named):
    add("vararg_named_%02d" % i, v + "\nreturn f", only="55")

# --- 12. multiple assignment / evaluation order --------------------------------------------------------------------------------------------------------
asg = ["a, b = b, a", "a, b, c = 1", "a, b, c = f()", "a, b, c = f(), 1", "a, b, c = 1, f()", "t[1], t[2] = t[2], t[1]",
       "t[a], t[b] = b, a", "a, t[a] = 1, 2", "t.x, t.y, t.z = 1, 2", "a, b = b", "a = a, b", "t[i], i = i, t[i]",
       "a, b, c, d = f(), f(), f()", "local x, y = a, b", "local x, y, z = a", "local x, y = f(), 1",
       "u, v = v, u", "t[f()], a = f(), f()", "t.a.b, t.c.d = 1, 2", "a, a = 1, 2", "local a, a = 1, 2",
       "t[#t + 1], t[#t + 2] = a, b", "_ENV.x, _ENV.y = 1, 2", "x, y = y, x"]
for i, a in enumerate(asg):
    add("assign_%02d" % i, "local a, b, c, d, i, t = ...\nlocal function f() return 1, 2, 3 end\nu, v = 1, 2\n" + a)

# --- 13. close / const / global (5.5) ------------------------------------------------------------------------------------------------------------------------------
close_progs = [
    "local x <close> = nil", "local x <close> = f() return 1", "do local x <close> = f() end",
    "local a <const>, b <close> = 1, f()", "local x <close> = f(); return x", "for i = 1, 2 do local x <close> = f() end",
    "local x <close> = f(); goto l; ::l::", "while true do local x <close> = f() break end",
    "local function g() local x <close> = f() return f() end", "local x <close> = f(); local y <close> = g()",
    "do local x <close> = f(); return end",
]
for i, c in enumerate(close_progs):
    add("close_%02d" % i, "local f, g = ...\n" + c)
consts = ["local x <const> = 10 return x", "local x <const> = 'str' return x .. x", "local x <const>, y <const> = 1, 2 return x + y",
          "local x <const> = nil return x", "local x <const> = true return not x", "local x <const> = 1.5 return -x",
          "local x <const> = f() return x", "local x <const> = {} return x", "local a <const> = 1; local b <const> = a + 1; return b",
          "local x <const> = 1; local function g() return x end return g", "local x <const> = math.maxinteger return x + 1",
          "local x <const> = 2^53 return x // 1", "local x <const> = 0.0 return x", "local s <const> = 'abc' return #s",
          "local x <const> = 10; for i = x, x do end", "local x <const> = 1; return x < 2, x == 1, x // 0.0"]
for i, c in enumerate(consts):
    add("const_%02d" % i, "local f = ...\n" + c)
globals_ = [
    "global x", "global x = 1", "global x, y = 1, 2", "global <const> x = 1", "global <const> *", "global *",
    "global function gf() end", "global x; x = 1; return x", "global x <const> = 5; return x", "global a, b, c = f()",
    "global *; y = 1", "global x; local function g() return x end", "global function gf(...) return ... end",
    "do global x; x = 1 end y = 2", "global x = 1; global y = x + 1; return y", "global <const> a, b = 1, 2; return a",
    "local global = 1; return global", "global = 1", "global.x = 1", "global(1)", "global function gf() global z; z = 1 end",
    "global x, y; x, y = y, x", "global x; function x() end", "global <const> x; return x",
]
for i, c in enumerate(globals_):
    add("global_%02d" % i, "local f = ...\n" + c, only="55")

# --- 14. table constructors ------------------------------------------------------------------------------------------------------------------------------------------------
add("tables", """
local a, b = ...
local t1 = {}
local t2 = {1, 2, 3}
local t3 = {x = 1, y = 2, [3] = 4, [a] = b, [1.5] = 2, ["k" .. a] = 1}
local t4 = {1, 2, x = 3, 4, [10] = 5, f = function() end, {1}, {x = {y = {}}}}
local t5 = {a, b, ...}
local t6 = {f(), f(), (f())}
local t7 = {[1] = 1, [2] = 2, n = 2; 3; 4,}
return t1, t2, t3, t4, t5, t6, t7
""")
for n in (5, 49, 50, 51, 120, 300):
    add("table_list_%03d" % n, "return {%s}" % ", ".join(str(i) for i in range(n)))
for n in (3, 17, 300):
    add("table_rec_%03d" % n, "local v = ...\nreturn {%s}" % ", ".join("k%d = v" % i for i in range(n)))
add("table_mixed_big", "local a = ...\nreturn {%s}" % ", ".join(("%d" % i) if i % 3 else ("k%d = %d" % (i, i)) for i in range(150)))
add("table_multret", "local f = ...\nreturn {f()}, {f(), f()}, {1, 2, f()}, {...}")

# --- 15. constants and registers near limits ---------------------------------------------------------------------------------------------------------------------------------------
add("many_consts_300", "local t = {}\n" + "\n".join("t[%d] = 'str%d'" % (i, i) for i in range(300)) + "\nreturn t")
add("many_consts_float", "local t = {}\n" + "\n".join("t[%d] = %d.5" % (i, i) for i in range(300)))
add("many_consts_idx", "local t = ...\n" + "\n".join("t.f%d = t.f%d" % (i, i + 1) for i in range(300)))
add("many_locals_190", "\n".join("local l%d = %d" % (i, i) for i in range(190)) + "\nreturn l0 + l189")
add("many_locals_198", "\n".join("local l%d" % i for i in range(198)))
add("many_upvalues", "\n".join("local u%d = %d" % (i, i) for i in range(60)) +
    "\nreturn function() return " + " + ".join("u%d" % i for i in range(60)) + " end")
add("deep_expr_regs", "local a = ...\nreturn " + "(a + " * 100 + "1" + ")" * 100)
add("concat_many", "local a = ...\nreturn " + " .. ".join(["a", "'x'", "1", "2.5"] * 12))
add("long_if_chain", "local x = ...\n" + "if x == 0 then x = 0\n" + "\n".join("elseif x == %d then x = %d" % (i, i * 2) for i in range(1, 80)) + "\nend")
add("long_jump", "local x = ...\nwhile x do\n" + "\n".join("x = x + %d" % i for i in range(3000)) + "\nend")
add("abs_lineinfo", "local x = 1\n" + "\n" * 200 + "x = x + 1\n" + "\n" * 300 + "x = x + 2\n" + "x = 1\n" * 140)
add("line_backwards", "local t = {\n1,\n2,\n}\nlocal f = function(\na,\nb)\nreturn a +\nb end\nreturn t,\nf(\n1,\n2)")

# --- 16. misc statements -----------------------------------------------------------------------------------------------------------------------------------------------------------------
add("misc_stat", """
local a <const> = 5
do end
;;;
local function f(a, b, ...) return a end
function t_x() end
local t = {}
function t.a() end
function t.a.b.c() end
function t:m() return self end
function t.a.b:m(x) return self, x end
return
""")
add("empty", "")
add("only_comment", "-- nothing here\n--[[ long\ncomment ]]")
add("return_forms", "local f = ...\nreturn\n")
for i, r in enumerate(["return 1", "return 1, 2", "return f()", "return (f())", "return f(), 1", "return ...", "return f(...)", "return nil", "return true, false",
                       "do return end", "do return 1 end", "return function() end", "return f() + 1", "return not f()", "return -f()",
                       "return f.x", "return f:m()", "return {f()}"]):
    add("ret_%02d" % i, "local f = ...\n" + r)
add("tail_calls", """
local f, g = ...
local function a() return f() end
local function b() return f(g()) end
local function c() return (f()) end
local function d() return f(), g() end
local function e() local x <close> = nil; return f() end
local function h() for i = 1, 2 do return f(i) end end
return a, b, c, d, e, h
""")
add("shadowing", """
local x = 1
local x = x + 1
do local x = x * 2; do local x = x + 3 end end
local function x() return x end
local x <const> = x
return x
""")
add("shifts_bitops", """
local a, b = ...
return a << 1, a >> 1, 1 << a, 1 >> a, a << -1, a >> -1, a << 255, a << 256, a & 1, 1 & a, a | 255, 256 | a, a ~ -1, -1 ~ a,
       ~a, a & b, a | b, a ~ b, a << b, a >> b, 3 << 2, 3 >> 2, -3 >> 1
""")
add("arith_imm", """
local a = ...
return a + 1, 1 + a, a - 1, 1 - a, a + 127, a + 128, a - 127, a - 128, a + -127, a * 2, 2 * a, a / 2, 2 / a, a // 2, a % 2,
       a ^ 2, 2 ^ a, a + 1.0, a + 1.5, a - 1.5, 1.5 + a, a * 2.0, a + 0.0, a - 0.0, a + -0.0, a + "1"
""")
add("eq_forms", """
local a = ...
return a == 1, 1 == a, a == 1.0, a == 1.5, a == "s", "s" == a, a == nil, nil == a, a == true, a == a, a ~= 1, a ~= "x",
       a == 127, a == 128, a == -127, a == -128, a == 1e100, a == 2^53
""")

# --- 5.4-specific chunks -----------------------------------------------------------------------------------------------------------------------------------------------------------------
brk = [
    "while a do if b then break end end", "while a do if b then break end; x = 1 end",
    "while a do if b then break; end end", "while a do if b then break ;;; end end",
    "while a do if b then break else x = 1 end end", "while a do if b then break elseif c then x = 1 end end",
    "while a do if b then x = 1 elseif c then break end end", "while a do if not b then break end end",
    "while a do if b and c then break end end", "while a do if b or c then break end end",
    "while a do if b == 1 then break end end", "while a do if f() then break end end",
    "while a do if nil then break end end", "while a do if false then break end end",
    "while a do if true then break end end", "while a do if 1 then break end end",
    "repeat if b then break end until a", "repeat local x = 1; if b then break end; f(function() return x end) until a",
    "for i = 1, 10 do if i then break end end", "for k, v in pairs(t) do if v then break end end",
    "for i = 1, 10 do for j = 1, 10 do if j then break end end end",
    "while a do if b then if c then break end end end", "while a do do if b then break end end end",
    "while a do local x = 1; f(function() return x end); if b then break end end",
    "while a do local x <close> = f(); if b then break end end",
    "while a do if b then break end if c then break end end", "while a do break end",
    "while a do local x = 1; f(function() return x end); break end",
    "while true do if a then break end if b then goto cont end ::cont:: end",
    "if a then break end", "do break end",
]
for i, c in enumerate(brk):
    add("break54_%02d" % i, "local a, b, c, f, t, x = ...\n" + c, only="54")
gt = [
    "do goto l1; local x; ::l1:: end", "do goto l1; local x; ::l1:: ; end",
    "do goto l1; local x; ::l1:: x = 1 end", "goto l1; do ::l1:: end",
    "::l1:: x = 1; goto l1", "::l1:: local y; f(function() return y end); goto l1",
    "do local y; f(function() return y end); goto l1 end ::l1::",
    "do local y; f(function() return y end); goto l1 end; x = 1 ::l1::",
    "while a do local y; f(function() return y end); if b then goto cont end; x = 1 ::cont:: end",
    "for i = 1, 3 do local y = i; f(function() return y end); goto cont; x = 1 ::cont:: end",
    "repeat local y; f(function() return y end); if b then goto cont end ::cont:: until a",
    "do goto a ::a:: goto b ::b:: end", "do ::a:: ::b:: goto a end", "goto l ::l:: ::m::",
    "do local x <close> = f(); goto l end ::l::", "do local x <close> = f(); goto l; ::l:: end",
    "::top:: do local x <close> = f(); if a then goto top end end",
    "::top:: do local x = 1; f(function() return x end); if a then goto top end end",
    "do ::l1:: end do ::l1:: end", "goto l1 ::l1::; ::l2::",
    "local x; goto l1 ::l1:: ", "do local x; goto l1 ::l1:: x = 1 end",
    "do goto l1 end local x ::l1::", "if a then goto l1 end local x = 1 ::l1::; return x",
]
for i, c in enumerate(gt):
    add("goto54_%02d" % i, "local a, b, c, f, t, x = ...\n" + c, only="54")
loops54 = [
    "for i = 1, 3 do end", "for i = 3, 1, -1 do end", "for i = 1.0, 3 do end", "for i = 1, 3.5 do end",
    "for i = 1, 3, 0.5 do end", "for i = a, b, c do end", "for i = math.maxinteger - 1, math.maxinteger do end",
    "for i = 1, 3 do local x = i; f(function() return x, i end) end", "for i = 1, 3 do i = i + 1 end",
    "for i, v in ipairs(t) do end", "for k in next, t do end", "for k, v, w, z in f do end",
    "for k, v in pairs(t) do local x <close> = v end", "for i = 1, 2 do for j = i, 2 do for k = j, 2 do end end end",
    "for i = 1, 0 do return i end", "for i = 1, 3 do goto c; ::c:: end", "for i = 1, 3 do if i then break end end",
    "for _ = 1, 3 do end", "for i = 1, #t do t[i] = i end", "for i = #t, 1, -1 do t[i] = nil end",
    "for k, v in f, t, 1, 2 do end", "for k, v in (f) do end",
]
for i, c in enumerate(loops54):
    add("for54_%02d" % i, "local a, b, c, f, t = ...\n" + c, only="54")
sf = [
    "local s = ...; return s:m()", "local s = ...; return s:m(1, 2)", "local s = ...; return s:a_very_long_method_name_that_is_longer_than_forty_chars()",
    "local s = ...; return s.x:m()", "local s = ...; return (s):m()", "local s = ...; return ('x'):rep(3)",
    "local s = ...; return s:m():n()", "local s = ...; s:m(s:n())", "return (...):m()", "local t = ...; return t.a.b.c:d(t.e)",
]
for i, c in enumerate(sf):
    add("self54_%02d" % i, c, only="54")
# the constant cache is shared by all functions of a chunk in 5.4: reuse across functions, ints vs. floats
add("kcache54_a", """
local a = ...
local x, y, z = 1000, 1000.0, "s"
local function f() return 1000, 1000.0, "s", 2^53, 2^53 | 0, -0.0, 0.0, 0 end
local function g() return 1000.0, 1000, "s", 2^63, -0.0, 0.0 end
return x, y, z, f, g, 9007199254740992, 9007199254740992.0, 0.0, -0.0, 0
""", only="54")
add("kcache54_b", """
local t = ...
local function f() return "a", "b", "c", 1.5, 2.5 end
local function g() return "c", "b", "a", 2.5, 1.5, 1.5 end
local function h() local function i() return "b", 1.5, nil, false, true end return i end
return t.a, t.b, t.c, f, g, h, 1.5, "a", nil, true, false
""", only="54")
cs = []
for n in range(300):
    cs.append("t.f%d = %d.5 + a" % (n, n))
add("kcache54_many", "local t, a = ...\n" + "\n".join(cs) + "\n"
    "local function g() return " + ", ".join("%d.5" % n for n in range(0, 300, 7)) + " end", only="54")
add("kcache54_zero", "local a = ...\nreturn 0.0, -0.0, 0, a + 0.0, a + -0.0, 0.0 == a, -0.0 == a, {0.0, -0.0, 0}", only="54")
add("kcache54_bigf", "local a = ...\nreturn 2^53, 2^53 + 1, 2^62, 2^63, 2^64, 1e300, -1e300, 1e15, 1e16, 123456789012345678, 123456789012345680.0", only="54")
# 5.4 'global' is an ordinary identifier
for i, c in enumerate(["global = 1", "global.x = 1", "global(1)", "local global = 1; return global", "function global() end",
                       "return global", "local t = {global = 1}; return t.global", "global, y = 1, 2",
                       "local global <const> = 1; return global", "global:m()", "return {global}"]):
    add("global54_%02d" % i, c, only="54")
# table constructors: flush every 50 items, extra args, hash sizes
add("cons54_flush", "local f = ...\nreturn {" + ", ".join(str(i) for i in range(260)) + "}", only="54")
add("cons54_flush_multret", "local f = ...\nreturn {" + ", ".join(str(i) for i in range(120)) + ", f()}", only="54")
add("cons54_hash", "local f = ...\nreturn {" + ", ".join("k%d = %d" % (i, i) for i in range(70)) + "}", only="54")
add("cons54_mixed", "local f = ...\nreturn {1, 2, x = 1, 3, [10] = 4, f(), y = 2, ...}", only="54")
add("cons54_big_extra", "local f = ...\nreturn {" + ", ".join(["1"] * 700) + "}", only="54")
add("vararg54", """
local function f(...) return ... end
local function g(a, b, ...) local x = select('#', ...) return x, a, b, ... end
local function h(...) local a, b = ... return function() return a, b end end
local function k(...) return {...}, {n = select('#', ...), ...} end
return f, g, h, k, ...
""", only="54")
add("tbc54", """
local f, g = ...
local function a() local x <close> = f() return 1 end
local function b() local x <close> = f() return g() end
local function c() local x <close> = f(); local y <close> = g(); do local z <close> = nil end end
local function d(...) local x <close> = f() return ... end
local function e() for i = 1, 2 do local x <close> = f(); if i then break end end end
return a, b, c, d, e
""", only="54")
add("const54", """
local f = ...
local a <const> = 10
local b <const> = "str"
local c <const> = a * 2 + 1
local d <const> = nil
local e <const>, g <const> = 1, f()
local function h() return a, b, c, d, e, g end
return a, b, c, d, e, g, h, a // 0.0, a == 10, b .. b, -c, ~a, not d
""", only="54")
add("lines54", "local a = ...\nlocal x =\n a\n +\n 1\n\n\n\nreturn\n x\n ,\n 2", only="54")
add("lineinfo54_abs", "local a = ...\n" + "\n" * 300 + "a = 1\n" + "\n" * 140 + "return a", only="54")
add("lineinfo54_many", "local a = ...\n" + "a = a + 1\n" * 400 + "return a", only="54")
add("closures54", """
local a, b = ...
local function f(x) return function(y) return function(z) return a + b + x + y + z end end end
local t = {}
for i = 1, 3 do t[i] = function() return i, a end end
while a do local c = a; t[#t + 1] = function() c = c + 1 return c end; if b then break end end
repeat local d = b; t[#t + 1] = function() return d end until d
return f, t
""", only="54")

# --- write ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------
def write(outdir, items):
    os.makedirs(outdir, exist_ok=True)
    for fn in os.listdir(outdir):
        if fn.endswith(".lua"):
            os.remove(os.path.join(outdir, fn))
    for name, text in sorted(items.items()):
        with open(os.path.join(outdir, name), "w", newline="\n") as f:
            f.write(text)
    print("wrote %d files to %s" % (len(items), os.path.normpath(outdir)))


write(OUT, {k: v for k, v in files.items() if k not in only54})
write(OUT54, {k: v for k, v in files.items() if k not in only55})
if len(files) - len(only54) < 300:
    sys.exit("fewer than 300 files generated")
