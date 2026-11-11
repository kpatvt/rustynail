-- JIT_FUZZ finding 1 (jit_diff, found 2026): compiled TFORCALL of an inlined `next`/`pairs` iterator does
-- not do the C-stack accounting (nCcalls) the interpreter does (OP_TFORCALL goes through luaD_call, which
-- raises "C stack overflow" near 200 nested C levels). Here the innermost frame's generic for fails in the
-- interpreter and in the reference Lua, but runs in compiled code.
-- expected (interpreter, reference lua 5.5): 198<TAB>591      JIT (always) printed: 198<TAB>594
-- run: RUSTYNAIL_JIT=always rnlua tests/jit_fuzz_repros/tfor_cstack_limit.lua
local depth = 0
local n = 0
local t1 = {1, 2, 3}
local function f1(p)
  depth = depth + 1
  pcall(f1)
  for k, e in pairs(t1) do
    n = n + 1
  end
end
f1(1)
print(depth, n)
