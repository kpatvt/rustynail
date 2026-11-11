# bench/corpus2: 32 more third-party-style and real programs (corpus2 study, 2026-11-07)

Every program is deterministic and prints the same output under PUC Lua 5.5 (`ref/55/build-seed0/lua`) and LuaJIT 2.1
(a distribution-packaged `luajit`), except `dev/real_dis_arm` (LuaJIT prints `#-0` where 5.5 prints `#0` for 3 of its 400 000 lines,
so its checksum line differs: LuaJIT's `-0` float semantics, not a bug). Each takes an optional workload argument `N`.
The default runs 0.3-0.75 s on PUC on the benchmark machine. The "small N" (also in each header) gives a run under ~0.05 s on PUC, for the GC-stress checks.
`<name>.expected` is the PUC 5.5 output at the default N. No program allocates more than ~50 MB or reads files or stdin.

## Split: dev / heldout (FLAG)

- **dev/** (19 programs): used in the corpus2 study for blocker analysis, ablations and knob retests.
- **heldout/** (13 programs): **never to be used to tune anything.** Only measure on it, or confirm a result found on dev.
  - The 11 written heldout programs come from a separate author (a separate subagent with a different brief) and a different
    style brief from dev: procedural / closures-as-objects / table-driven dispatch / prototype cloning / Lua 5.1 idioms,
    against dev's library-style colon-method OOP.
  - The authors were not shown the JIT docs or the JIT itself.
  - In the study, the heldout set was used only for the timing table, for one confirmation of a dev-derived claim
    (`stream_closures`, see the study report), and for the confirmation of knob results.
  - Its abort statistics were collected along with dev's for the cross-program signature matrix. They were not used to
    pick ablations.
- **Overlap warning:** the 4 `real_dis_*` programs are one code base (LuaJIT's disassemblers, one author) driven by the
  same driver pattern. `dev` has x86/x64 and ARM; `heldout` has MIPS and PPC. This split leaks style: treat the two
  heldout `real_dis_*` programs as weak held-out evidence.

## (a) Real code found on the benchmark machine

The whole filesystem was searched outside the repo and `ref/` (and outside ~/.cache copies of the repo): 151 `.lua` files.
- Almost all are host-API scripts that cannot run without their host:
  - wireplumber (80), gegl (24, mostly FFI cairo bindings), mpv (17), vlc (7), gimp (2), ghidra (1), brltty (1); the other 19 are LuaJIT's own `jit/` modules.
- No luarocks tree, neovim runtime, awesome, nmap, wireshark, texlive or lua-* library packages are installed.
- The only self-contained algorithmic Lua is LuaJIT's own `jit/` modules (the `jit/` directory of a LuaJIT 2.1 distribution package, MIT license, (c) Mike Pall). Of these, the disassemblers are large pure-Lua programs:
  table-driven decoders with string formatting and pattern matching.

**Not unmodified.** That LuaJIT build ships the modules in an extended dialect. Not one of `dis_x86/arm/arm64/mips/ppc.lua`
parses under PUC 5.5. The dialect has:
- `!=`, compound assignment (`+=`, `..=`, `>>=`, ...), the ternary `c ? a : b`, and `??`;
- 32-bit bit operators (`&`, `|`, `<<`, `>>`, and `~>>` = arithmetic shift, int32 results, shift counts mod 32).

`vendor/jit/*.lua` are therefore **ported**, with semantics unchanged:
- **Mechanical syntax desugaring:** `tools/desugar.py`, kept with the study data.
  - `!=` -> `~=`, `x op= e` -> `x = x op (e)`, `c ? a : b` -> `((c) and (a) or (b))`.
  - The two sites whose true branch is `nil` were rewritten by hand as `if` statements.
- **`~>>`** -> a local `sar32(v, n)` helper added at the top of the file (marked `[corpus2 port]`).
- **One shift count masked in `dis_arm.lua`** (`(sh0-1) & 31`; the dialect masks counts to 5 bits).
- All other bit operators are left as native Lua 5.3+ operators: the operands are non-negative 32-bit values.
- `bit.tohex` / `bit.ror` come from a small shim in each driver (`package.preload.bit` when `require "bit"` fails).

**Equivalence evidence:**
- Each driver was run twice: with the ORIGINAL dialect files under the system LuaJIT, and with the ported files under PUC 5.5.
- The outputs are byte-identical for x86/x64, MIPS and PPC (100 000-400 000 disassembled instructions each).
- ARM matches except the 3 `-0` lines noted above.
- The 5.5 port prints the same under LuaJIT too.
- `dis_arm64.lua` was not ported (64-bit immediates, `??`, many width-sensitive operations): too risky to port without semantic drift.

| split | program | real code used | workload (driver) |
|---|---|---|---|
| dev | real_dis_x86 | `vendor/jit/dis_x86.lua`, `dis_x64.lua` | 2 x 150 blocks of 2000 bytes of pseudo-random x86 code biased to common encodings, 32- and 64-bit mode; mnemonic histogram + checksum |
| dev | real_dis_arm | `vendor/jit/dis_arm.lua` | 200 blocks of 2000 pseudo-random A32 words (mostly cond AL) |
| heldout | real_dis_mips | `vendor/jit/dis_mips.lua`, `dis_mipsel.lua` | 2 x 100 blocks of 2000 MIPS32 words, BE and LE |
| heldout | real_dis_ppc | `vendor/jit/dis_ppc.lua` | 200 blocks of 2000 PPC words |

License: MIT (LuaJIT). The original copyright headers are kept in the vendored files. The drivers are new code.

## (b) Newly written third-party-style programs

These were written for this study by two separate authors (automated authoring), one per split. Each author worked from a
style brief and was not shown the JIT. The programs are original code, not copies of any library. Lines are file lengths.

| split | program | lines | style / what it imitates | small N |
|---|---|---|---|---|
| dev | bignum_arith | 237 | arbitrary precision on base-10^7 limb arrays: factorials, Fibonacci, e/pi spigot | 2 |
| dev | coro_generators | 225 | coroutine generators: permutations (PiL), tree walkers, prime sieve as a chain of filter coroutines, token generator | 2 |
| dev | csv_report | 165 | CSV with quoted fields: generate, parse, group-by/aggregate, sort, formatted report | 20 |
| dev | diff_levenshtein | 208 | Levenshtein / Damerau, LCS line diff with unified output, fuzzy dictionary matching | 2 |
| dev | ecs_world | 235 | entity-component-system game loop, systems, spawn/despawn, uniform-grid collision | 2 |
| dev | event_promise | 235 | event emitter (on/off/once/emit varargs) + promise library (then/catch/all) on a fake event loop | 3 |
| dev | expr_compiler | 260 | tokenizer + Pratt parser, AST -> closures and -> stack bytecode with an interpreter loop | 2 |
| dev | iter_pipeline | 257 | luafun-style lazy iterator library (closure `next` functions, method chaining, `for v in it.next`) | 5 |
| dev | lua_parser | 299 | recursive-descent parser for a Lua subset, pretty-printer, constant folder | 1 |
| dev | memo_cache | 246 | memoize (nested tables / string keys), LRU with a doubly linked list, recursive DP problems | 2 |
| dev | oop_mixins | 236 | class library: 3-4 level `__index` chains, super calls, mixins, `__tostring/__eq/__lt`, `__call` constructors with varargs | 2 |
| dev | particles_physics | 178 | 2D Verlet particles, springs, spatial hash, vector helpers on plain tables | 2 |
| dev | regex_nfa | 225 | regex parser -> Thompson NFA -> state-set simulation | 2 |
| dev | template_gsub | 161 | `${var}` / `{{#each}}` templating with filters, HTML escaping via gsub tables, invoices | 3 |
| dev | text_adventure | 220 | text adventure: rooms, inventory, verb/noun parser with synonyms, NPC dialogue state machine | 3 |
| dev | varargs_util | 204 | printf-like formatters, curry/partial/compose, pack/unpack, `select` loops, vararg forwarding | 3 |
| dev | wordfreq_wrap | 167 | word frequency, wrap/justify, capitalisation, naive stemming with gsub | 3 |
| heldout | bigint_rsa | 303 | hobby bignum (base-10000 little-endian digit arrays), long division, modpow, toy RSA with Miller-Rabin | 1 |
| heldout | dijkstra_routes | 248 | road network: Dijkstra with a hand-written heap of {dist,node}, BFS/DFS, all-pairs subset | 2 |
| heldout | forth_vm | 217 | toy Forth: outer interpreter, dictionary of Lua primitives and compiled colon words, IF/DO/BEGIN | 1 |
| heldout | log_analyzer | 162 | access-log parsing with match/gmatch, aggregation per status/path/hour, percentiles, top-N | 1 |
| heldout | markov_text | 192 | order-2 word Markov chain, generation, character n-grams, similarity score | 1 |
| heldout | prototype_objects | 251 | prototype OOP by cloning, 4-level `__index` delegation, mixins by copying (bank/shop) | 1 |
| heldout | roguelike_turns | 299 | roguelike: map generation, closure-object monsters with string-state AI, ray-cast FOV, turn scheduler | 1 |
| heldout | scheme_interp | 299 | Scheme interpreter: reader, environments with parent links, eval/apply, special forms | 1 |
| heldout | stream_closures | 252 | stream library of closure stages (next-functions, no coroutines): map/filter/flatmap/window/distinct | 1 |
| heldout | sudoku_solver | 278 | constraint propagation + backtracking sudoku, N-queens, knapsack DP | 1 |
| heldout | vending_fsm | 271 | table-driven FSMs keyed by state/event strings: vending machine, traffic light, HTTP-like parser | 1 |

Real-code drivers take `small N = 0.1`.
