#!/usr/bin/env python3
"""Syntax-only desugaring of the extended-LuaJIT dialect used by the system's /usr/share/luajit-2.1/jit/*.lua
into standard Lua 5.3+ syntax:  a != b -> a ~= b;  x op= e -> x = x op (e);  c ? a : b -> ((c) and (a) or (b)).
Sites where the true branch can be nil/false are reported (WARN) and must be fixed by hand.
Usage: desugar.py in.lua out.lua"""
import re, sys

KW_STMT = {"then", "end", "else", "elseif", "do", "local", "return", "if", "while", "for", "until", "repeat", "function", "break"}
BINOPS = {"..", "+", "-", "*", "/", "%", "^", "and", "or", "==", "~=", "!=", "<", ">", "<=", ">=", "&", "|", "~", "<<", ">>", ":", "?", "??", "//"}


def tokenize(src):
    toks = []  # (kind, text)  kinds: ws, nl, com, str, name, num, op
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == "\n":
            toks.append(("nl", c)); i += 1; continue
        if c in " \t\r":
            j = i
            while j < n and src[j] in " \t\r":
                j += 1
            toks.append(("ws", src[i:j])); i = j; continue
        if src.startswith("--", i):
            m = re.match(r"--\[(=*)\[", src[i:])
            if m:
                close = "]" + m.group(1) + "]"
                j = src.index(close, i) + len(close)
            else:
                j = src.find("\n", i)
                j = n if j < 0 else j
            toks.append(("com", src[i:j])); i = j; continue
        m = re.match(r"\[(=*)\[", src[i:])
        if m:
            close = "]" + m.group(1) + "]"
            j = src.index(close, i) + len(close)
            toks.append(("str", src[i:j])); i = j; continue
        if c in "\"'":
            j = i + 1
            while src[j] != c:
                j += 2 if src[j] == "\\" else 1
            toks.append(("str", src[i:j + 1])); i = j + 1; continue
        m = re.match(r"[A-Za-z_][A-Za-z0-9_]*", src[i:])
        if m:
            toks.append(("name", m.group(0))); i += len(m.group(0)); continue
        m = re.match(r"0[xX][0-9a-fA-F]+|\d+\.?\d*(?:[eE][-+]?\d+)?", src[i:])
        if m:
            toks.append(("num", m.group(0))); i += len(m.group(0)); continue
        for op in ("~>>", "..=", ">>=", "<<=", "...", "??", "!=", "+=", "-=", "*=", "/=", "&=", "|=", "==", "~=", "<=", ">=", "<<", ">>", "//", "::", ".."):
            if src.startswith(op, i):
                toks.append(("op", op)); i += len(op); break
        else:
            toks.append(("op", c)); i += 1
    return toks


def sig(toks, i, step):
    """index of next significant token from i (exclusive) in direction step"""
    j = i + step
    while 0 <= j < len(toks) and toks[j][0] in ("ws", "com", "nl"):
        j += step
    return j


def expr_end(toks, i):
    """i = first token of an expression; return index one past its last token (lowest-precedence expression)."""
    depth = 0
    j = i
    last_sig = None
    while j < len(toks):
        k, t = toks[j]
        if k in ("ws", "com"):
            j += 1; continue
        if k == "nl":
            if depth == 0 and last_sig is not None:
                nx = sig(toks, j, 1)
                if nx >= len(toks):
                    return j
                nk, nt = toks[nx]
                # continuation if previous token was a binary op or next token is a binary op
                if not ((last_sig[0] == "op" and last_sig[1] in BINOPS | {"(", ",", "["}) or (last_sig[0] == "name" and last_sig[1] in ("and", "or")) or
                        (nk == "op" and nt in BINOPS) or (nk == "name" and nt in ("and", "or"))):
                    return j
            j += 1; continue
        if k == "op" and t in "([{":
            depth += 1
        elif k == "op" and t in ")]}":
            if depth == 0:
                return j
            depth -= 1
        elif depth == 0 and ((k == "op" and t in (",", ";", "=", "+=", "-=", "*=", "/=", "..=", ">>=", "<<=", "&=", "|=", ":")) or (k == "name" and t in KW_STMT)):
            if not (k == "op" and t == ":" and False):
                return j
        last_sig = (k, t)
        j += 1
    return j


def expr_start(toks, i):
    """i = the operator token ('?' or 'op='); return index of the first token of the expression ending before i."""
    depth = 0
    j = i - 1
    first = i
    while j >= 0:
        k, t = toks[j]
        if k == "nl" and depth == 0 and first < i:
            pv = sig(toks, j, -1)
            fk, ft = toks[first]
            if pv < 0:
                return first
            pk, pt = toks[pv]
            if not ((pk == "op" and pt in BINOPS) or (pk == "name" and pt in ("and", "or", "not")) or
                    (fk == "op" and ft in BINOPS) or (fk == "name" and ft in ("and", "or"))):
                return first
        if k in ("ws", "com", "nl"):
            j -= 1; continue
        if k == "op" and t in ")]}":
            depth += 1
        elif k == "op" and t in "([{":
            if depth == 0:
                return first
            depth -= 1
        elif depth == 0 and ((k == "op" and t in (",", ";", "=", "+=", "-=", "*=", "/=", "..=", ">>=", "<<=", "&=", "|=")) or (k == "name" and t in KW_STMT)):
            return first
        first = j
        j -= 1
    return first


def lhs_start(toks, o):
    j = sig(toks, o, -1)
    first = o
    while j >= 0:
        k, t = toks[j]
        if k == "op" and t == "]":
            d = 0
            while True:
                if toks[j][1] == "]": d += 1
                elif toks[j][1] == "[": d -= 1
                if d == 0: break
                j -= 1
            first = j; j = sig(toks, j, -1); continue
        if (k == "name" and t not in KW_STMT and t not in ("and", "or", "not")) or (k == "op" and t == "."):
            first = j
            nj = sig(toks, j, -1)
            if any(toks[x][0] == "nl" for x in range(nj + 1, j)) or (nj >= 0 and toks[nj][0] not in ("op",) and toks[j][0] == "name" and toks[nj][1] != "."):
                break
            j = nj; continue
        break
    return first


def text(toks, a, b):
    return "".join(t for _, t in toks[a:b])


def desugar(src, fname):
    toks = tokenize(src)
    toks = [("op", "~=") if (k == "op" and t == "!=") else (k, t) for k, t in toks]
    # ternaries, innermost-last: repeat until none
    while True:
        qs = [i for i, (k, t) in enumerate(toks) if k == "op" and t == "?"]
        if not qs:
            break
        q = qs[-1]
        cs = expr_start(toks, q)
        # true branch: up to ':' at depth 0
        depth, j = 0, q + 1
        while True:
            k, t = toks[j]
            if k == "op" and t in "([{":
                depth += 1
            elif k == "op" and t in ")]}":
                depth -= 1
            elif k == "op" and t == ":" and depth == 0:
                break
            j += 1
        colon = j
        fe = expr_end(toks, sig(toks, colon, 1))
        C, A, B = text(toks, cs, q).strip(), text(toks, q + 1, colon).strip(), text(toks, colon + 1, fe).strip()
        if A in ("nil", "false") or re.search(r"\[[^\]]*\]$", A):
            print(f"WARN {fname}: true branch may be nil/false: {C} ? {A} : {B}", file=sys.stderr)
        new = f"(({C}) and ({A}) or ({B}))"
        toks[cs:fe] = [("raw", new)]
    # compound assignments
    while True:
        cs_ = [i for i, (k, t) in enumerate(toks) if k == "op" and t in ("+=", "-=", "*=", "/=", "..=", ">>=", "<<=", "&=", "|=")]
        if not cs_:
            break
        o = cs_[0]
        op = toks[o][1][:-1]
        ls = lhs_start(toks, o)
        re_ = expr_end(toks, sig(toks, o, 1))
        L = text(toks, ls, o).strip()
        R = text(toks, o + 1, re_).strip()
        toks[ls:re_] = [("raw", f"{L} = {L} {op} ({R})")]
    if any(k == "op" and t == "??" for k, t in toks):
        print(f"WARN {fname}: '??' left for manual fix", file=sys.stderr)
    return "".join(t for _, t in toks)


if __name__ == "__main__":
    src = open(sys.argv[1]).read()
    out = desugar(src, sys.argv[1])
    open(sys.argv[2], "w").write(out)
