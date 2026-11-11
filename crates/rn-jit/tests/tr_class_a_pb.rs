//! P-B: the library-call helpers of TR1 (`LIB` = `lib_core` of `tr::asm`) and the single-probe string interning behind `SNEW` /
//! `CONCAT` in isolation, on a real state.
//!
//! Same contract as `tr_class_a_f3b.rs`: a helper that reports 0 has **done nothing** and does so exactly when the interpreter's
//! `check_gc` right after the same allocation would step the collector (or the allocation could fail, or the case is one the
//! interpreter's general path owns); otherwise it returns what the C function returns with the interpreter's own accounting.
//! Detectors of the mutants `lib_bytes_small`, `lib_no_gc_check`, `lib_kind_skip`, `sub_start_off`, `byte_plus1_skip`,
//! `strmeth_no_check`, `type_closure_unchecked`, `tostr_meta_skip`.
#![cfg(feature = "trace")]

use rn_core::state::{LuaState, State};
use rn_core::value::{ctb, LuaType, Value, LUA_VLNGSTR, LUA_VSHRSTR};
use rn_jit::tr::asm::lib_core;
use rn_jit::tr::ir::lib;

fn state() -> LuaState {
    let mut st = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, rn_jit::Mode::Off);
    st.heap().gc_set_stp(0);
    st.heap().gc_set_threshold(usize::MAX);
    let (core, typefn) = rn_std::lib_base::jit_libfns();
    let [sub, byte, chr] = rn_std::lib_string::jit_libfns();
    let [ins, rem] = rn_std::lib_table::jit_libfns();
    let [mty, fmod, toint] = rn_std::lib_math::jit_libfns();
    let [tostr, rawlen, rawequal, rawget, rawset, tonum] = core;
    rn_core::jit_hooks::set_libfns(
        [sub, byte, chr, mty, tostr, rawlen, rawequal, rawget, rawset, ins, rem, fmod, toint, tonum],
        typefn,
    );
    st
}

fn sref(st: &mut State, s: &[u8]) -> u64 {
    st.new_str(s).expect("string").as_ptr() as u64
}

/// The bytes of the string `p` returned by a helper (`long`: a long string).
fn text(p: u64, long: bool) -> Vec<u8> {
    let tag = if long { ctb(LUA_VLNGSTR) } else { ctb(LUA_VSHRSTR) };
    // SAFETY: the helper returned a live string of the kind asked for.
    let v = unsafe { std::mem::transmute::<[u64; 2], Value>([u64::from(tag), p]) };
    v.as_strref().expect("string").get().as_bytes().to_vec()
}

fn call(st: &mut State, x: u32, args: [u64; 3]) -> u64 {
    // SAFETY: the operands are live strings of the kinds the flags say, built by the tests.
    unsafe { lib_core(st, x, args) }
}

const S: &[u8] = b"hello world";

/// `string.sub` with the C function's index rules (checked against hand-computed results).
#[test]
fn lib_sub_follows_the_c_function() {
    let cases: &[(i64, i64, &[u8])] = &[
        (1, -1, b"hello world"),
        (2, 3, b"el"),
        (3, -3, b"llo wor"),
        (-3, -1, b"rld"),
        (0, 2, b"he"),      // 0 starts at 1
        (-100, 2, b"he"),   // before the start
        (5, 100, b"o world"), // after the end
        (7, 6, b""),
        (12, 20, b""),
        (4, 0, b""),
        (-5, -5, b"w"),
        (1, 1, b"h"),
        (11, 11, b"d"),
    ];
    let mut st = state();
    let s = sref(&mut st, S);
    for &(i, j, want) in cases {
        let r = call(&mut st, lib::SUB, [s, i as u64, j as u64]);
        assert_ne!(r, 0, "sub({i},{j}) did nothing");
        assert_eq!(text(r, false), want, "sub({i},{j})");
    }
}

/// A long operand and a long result (flag bits 8 and 9); the recorded kind of the result is compared.
#[test]
fn lib_sub_long_strings_and_the_result_kind() {
    let mut st = state();
    let big: Vec<u8> = (0..200u32).map(|i| b'a' + (i % 26) as u8).collect();
    let s = sref(&mut st, &big);
    // 100 bytes: a long result, recorded long
    let r = call(&mut st, lib::SUB | 1 << 8 | 2 << 8, [s, 1, 100]);
    assert_ne!(r, 0);
    assert_eq!(text(r, true), &big[..100]);
    // 40 bytes: short, recorded long -> the other kind: did nothing
    let t = st.heap().total_bytes();
    assert_eq!(call(&mut st, lib::SUB | 1 << 8 | 2 << 8, [s, 1, 40]), 0, "short result recorded long");
    assert_eq!(call(&mut st, lib::SUB | 1 << 8, [s, 1, 41]), 0, "long result recorded short");
    assert_eq!(st.heap().total_bytes(), t, "nothing allocated");
    // 40 bytes recorded short
    let r = call(&mut st, lib::SUB | 1 << 8, [s, 3, 42]);
    assert_ne!(r, 0);
    assert_eq!(text(r, false), &big[2..42]);
}

/// `string.byte`: one byte (plus one), 0 for anything else.
#[test]
fn lib_byte_is_the_byte_plus_one() {
    let mut st = state();
    let s = sref(&mut st, S);
    for (i, want) in [(1i64, b'h'), (5, b'o'), (11, b'd'), (-1, b'd'), (-11, b'h')] {
        assert_eq!(call(&mut st, lib::BYTE, [s, i as u64, 0]), u64::from(want) + 1, "byte({i})");
    }
    for i in [0i64, 12, 100, -12, -100] {
        // (0 is posrelat'ed to 1 but the end position is 0: no byte)
        assert_eq!(call(&mut st, lib::BYTE, [s, i as u64, 0]), 0, "byte({i}) is not one byte");
    }
    let z = sref(&mut st, &[0u8, 1, 2]);
    assert_eq!(call(&mut st, lib::BYTE, [z, 1, 0]), 1, "a zero byte is 0 + 1");
}

/// `string.char`: one byte; above 255 did nothing.
#[test]
fn lib_char_makes_one_byte_strings() {
    let mut st = state();
    for c in [0u64, 65, 255] {
        let r = call(&mut st, lib::CHAR, [c, 0, 0]);
        assert_ne!(r, 0);
        assert_eq!(text(r, false), vec![c as u8]);
        // the interned object
        assert_eq!(r, sref(&mut st, &[c as u8]));
    }
    assert_eq!(call(&mut st, lib::CHAR, [256, 0, 0]), 0);
    assert_eq!(call(&mut st, lib::CHAR, [u64::MAX, 0, 0]), 0);
}

/// B3-6: one-byte results (`CHAR`, a one-byte `SUB`) are the fixed preallocated strings: no bytes added, the very object the interpreter
/// finds, and the helper still answers (the string exists, nothing is allocated).
#[test]
fn lib_one_byte_results_allocate_nothing() {
    let mut st = state();
    let s = sref(&mut st, S);
    let total = st.heap().total_bytes();
    st.heap().gc_set_threshold(total + 1);
    for c in [0u64, 65, 200, 255] {
        let r = call(&mut st, lib::CHAR, [c, 0, 0]);
        assert_ne!(r, 0);
        assert_eq!(text(r, false), vec![c as u8]);
        assert_eq!(st.heap().total_bytes(), total, "no allocation");
    }
    let r = call(&mut st, lib::SUB, [s, 2, 2]);
    assert_eq!(text(r, false), b"e".to_vec());
    assert_eq!(st.heap().total_bytes(), total);
    assert_eq!(r, sref(&mut st, b"e"));
}

/// The bytes the interpreter's string creation adds for `s`, measured in a state of its own.
fn interpreter_bytes(s: &[u8]) -> usize {
    let mut r = state();
    let t0 = r.heap().total_bytes();
    r.new_str(s).unwrap();
    r.heap().total_bytes() - t0
}

/// `SUB` / `CHAR` / `TYPENAME` / `TOSTR` allocate exactly when the collector stays idle behind the allocation, with the interpreter's
/// accounting; an interned result adds nothing.
#[test]
fn lib_helpers_exit_exactly_when_a_step_would_be_due() {
    struct Case {
        x: u32,
        args: [u64; 3],
        want: Vec<u8>,
        long: bool,
    }
    let big: Vec<u8> = (0..120u32).map(|i| b'A' + (i % 26) as u8).collect();
    let mut cases: Vec<Box<dyn Fn(&mut State) -> Case>> = Vec::new();
    cases.push(Box::new(|st| Case { x: lib::SUB, args: [sref(st, S), 2, 4], want: b"ell".to_vec(), long: false }));
    let b2 = big.clone();
    cases.push(Box::new(move |st| Case {
        x: lib::SUB | 1 << 8 | 2 << 8,
        args: [sref(st, &b2), 1, 80],
        want: b2[..80].to_vec(),
        long: true,
    }));
    // (`CHAR` makes one-byte strings: the 256 fixed ones of B3-6 add nothing, see `lib_one_byte_results_allocate_nothing`)
    cases.push(Box::new(|_| Case { x: lib::TYPENAME, args: [8, 0, 0], want: b"integer".to_vec(), long: false }));
    cases.push(Box::new(|_| Case { x: lib::TYPENAME, args: [9, 0, 0], want: b"float".to_vec(), long: false }));
    cases.push(Box::new(|_| Case { x: lib::TOSTR, args: [123456789u64, 0, 0], want: b"123456789".to_vec(), long: false }));
    cases.push(Box::new(|_| Case {
        x: lib::TOSTR | 4 << 8,
        args: [2.5f64.to_bits(), 0, 0],
        want: b"2.5".to_vec(),
        long: false,
    }));
    for (k, mk) in cases.iter().enumerate() {
        let mut st = state();
        let case = mk(&mut st);
        let bytes = interpreter_bytes(&case.want);
        assert!(bytes > 0, "case {k}");
        let total = st.heap().total_bytes();
        for thr in [total + bytes - 1, total + bytes, total, 0] {
            st.heap().gc_set_threshold(thr);
            assert_eq!(call(&mut st, case.x, case.args), 0, "case {k} threshold {thr}: must exit");
            assert_eq!(st.heap().total_bytes(), total, "case {k}: the exit allocated");
        }
        st.heap().gc_set_threshold(total + bytes + 1);
        let r = call(&mut st, case.x, case.args);
        assert_ne!(r, 0, "case {k}: must allocate");
        assert_eq!(st.heap().total_bytes(), total + bytes, "case {k}: accounting");
        assert_eq!(text(r, case.long), case.want, "case {k}: contents");
        assert!(!st.heap().gc_due(), "case {k}: the collector must stay idle");
        if !case.long {
            // the interned object: a second call adds nothing, and only `total < threshold` matters
            let total = st.heap().total_bytes();
            st.heap().gc_set_threshold(total + 1);
            assert_eq!(call(&mut st, case.x, case.args), r, "case {k}: present string");
            st.heap().gc_set_threshold(total);
            assert_eq!(call(&mut st, case.x, case.args), 0, "case {k}: present string, threshold reached");
        }
    }
}

/// The memory limit: an allocation that would fail is not attempted.
#[test]
fn lib_helpers_exit_when_the_limit_would_fail() {
    let mut st = state();
    let s = sref(&mut st, S);
    let need = interpreter_bytes(b"ell");
    let total = st.heap().total_bytes();
    st.heap().mem_mut().set_limit(total + need - 1);
    assert_eq!(call(&mut st, lib::SUB, [s, 2, 4]), 0);
    assert_eq!(st.heap().total_bytes(), total);
    st.heap().mem_mut().set_limit(total + need);
    assert_ne!(call(&mut st, lib::SUB, [s, 2, 4]), 0);
    st.heap().mem_mut().set_limit(usize::MAX);
}

/// `STRMETH`: the light C function under the name in the string metatable's `__index` table; anything else did nothing.
#[test]
fn lib_strmeth_finds_light_c_functions_only() {
    let mut st = state();
    let sub = sref(&mut st, b"sub");
    let want = rn_std::lib_string::jit_libfns()[0] as u64;
    assert_eq!(call(&mut st, lib::STRMETH, [sub, 0, 0]), want);
    // an absent name
    let nope = sref(&mut st, b"no_such_method");
    assert_eq!(call(&mut st, lib::STRMETH, [nope, 0, 0]), 0);
    // a name bound to a Lua function (user-replaced): not a light C function
    let status = st.load_buffer(b"string.sub = function() return 1 end", b"=t", None);
    assert_eq!(status, rn_core::status::Status::Ok);
    let f = st.top() - 1;
    assert_eq!(st.pcall(f, 0, 0), rn_core::status::Status::Ok);
    assert_eq!(call(&mut st, lib::STRMETH, [sub, 0, 0]), 0, "a Lua function under the name");
    // no `__index` table: the metatable is gone
    st.set_type_metatable(LuaType::String, None);
    let len = sref(&mut st, b"len");
    assert_eq!(call(&mut st, lib::STRMETH, [len, 0, 0]), 0, "no string metatable");
}

/// `TYPE`: the callee must be `type` itself; the result is the closure's upvalue string (no allocation).
#[test]
fn lib_type_checks_the_closure() {
    let mut st = state();
    let g = st.globals();
    let key = st.new_str(b"type").unwrap();
    let tv = g.get_str(key);
    let tc = tv.as_cclosure().expect("type is a C closure");
    let total = st.heap().total_bytes();
    for v in [Value::NIL, Value::int(1), Value::float(1.5), Value::TRUE] {
        let r = call(&mut st, lib::TYPE, [tc.as_ptr() as u64, v.type_of() as u64, 0]);
        let want = match v.type_of() as usize {
            0 => "nil",
            1 => "boolean",
            _ => "number",
        };
        assert_eq!(text(r, false), want.as_bytes());
    }
    assert_eq!(st.heap().total_bytes(), total, "no allocation");
    // another C closure (a `string.gmatch` iterator) is not `type`
    let status = st.load_buffer(b"return string.gmatch('a', 'a')", b"=t", None);
    assert_eq!(status, rn_core::status::Status::Ok);
    let f = st.top() - 1;
    assert_eq!(st.pcall(f, 1, 0), rn_core::status::Status::Ok);
    let it = st.get(st.top() - 1);
    let cc = it.as_cclosure().expect("gmatch iterator is a C closure");
    assert_eq!(call(&mut st, lib::TYPE, [cc.as_ptr() as u64, 0, 0]), 0, "another closure");
    // an upvalue index beyond the closure's
    assert_eq!(call(&mut st, lib::TYPE, [tc.as_ptr() as u64, 99, 0]), 0);
}

/// `TOSTR` leaves a number with a metatable to the interpreter (`__tostring` / `__name` could apply).
#[test]
fn lib_tostr_leaves_numbers_with_metatables_alone() {
    let mut st = state();
    let r = call(&mut st, lib::TOSTR, [7, 0, 0]);
    assert_eq!(text(r, false), b"7");
    let mt = st.new_table().expect("table");
    st.set_type_metatable(LuaType::Number, Some(mt));
    assert_eq!(call(&mut st, lib::TOSTR, [8, 0, 0]), 0);
    st.set_type_metatable(LuaType::Number, None);
    assert_ne!(call(&mut st, lib::TOSTR, [8, 0, 0]), 0);
}

/// The single-probe interning behind the string helpers resurrects a string that is dead but not yet swept, like `new_str`.
#[test]
fn probe_commit_returns_the_interned_string() {
    let mut st = state();
    let heap = st.heap();
    let (h, found, bytes) = heap.jit_str_probe(b"probe-me");
    assert!(found.is_null());
    assert!(bytes.is_some_and(|b| b > 0));
    // SAFETY: nothing happened since the probe.
    let a = unsafe { heap.jit_str_commit(b"probe-me", h, found) }.expect("string");
    let (h2, found2, bytes2) = heap.jit_str_probe(b"probe-me");
    assert_eq!((found2 as usize, bytes2), (a.as_ptr() as usize, Some(0)));
    // SAFETY: as above.
    let b = unsafe { heap.jit_str_commit(b"probe-me", h2, found2) }.expect("string");
    assert_eq!(a.as_ptr(), b.as_ptr());
    assert_eq!(heap.new_str(b"probe-me").unwrap().as_ptr(), a.as_ptr(), "the very object `new_str` finds");
}

/// P-O: `CCALLS` (the guard of an inlined `pcall`) answers 1 while `ccalls + k + 8 < LUAI_MAXCCALLS` and 0 beyond (the exit then
/// lets the interpreter's `pcall` raise "C stack overflow" at the right depth). Mutant `ccalls_guard_off`.
#[test]
fn lib_ccalls_follows_the_c_stack_depth() {
    let mut st = state();
    let max = rn_core::state::LUAI_MAXCCALLS;
    for k in [1u64, 2, 3] {
        for used in 0..max + 4 {
            let raw = st.ncalls_raw();
            st.set_ncalls_raw((raw & 0xffff_0000) | used);
            let want = u64::from(u64::from(used) + k + 8 < u64::from(max));
            assert_eq!(call(&mut st, lib::CCALLS, [k, k, 0]), want, "ccalls {used}, k {k}");
            st.set_ncalls_raw(raw);
        }
    }
}

/// BUNDLE-A `libfind`: the plain `string.find` helper (`FIND` / `FINDS`): 1 = not found, `pos + 2` found, with the library's start rules (negative,
/// beyond the end, empty pattern); `FINDS` answers 0 (the trace exits) for a pattern with specials. Detectors of the mutants `find_specials_skip`,
/// `find_init_neg_skip`.
#[test]
fn lib_find_follows_the_library() {
    let mut st = state();
    let s = sref(&mut st, S); // "hello world", 11 bytes
    let cases: &[(&[u8], i64, Option<usize>)] = &[
        (b"o", 1, Some(4)),
        (b"o", 6, Some(7)),
        (b"o", 9, None),
        (b"", 1, Some(0)),
        (b"", 12, Some(11)), // #s + 1 is allowed
        (b"", 13, None),     // beyond #s + 1 fails
        (b"o", -5, Some(7)),
        (b"o", -100, Some(4)),
        (b"o", 0, Some(4)),
        (b"d", -1, Some(10)),
        (b"hello world!", 1, None),
        (b"hello world", 1, Some(0)),
        (b"world", 7, Some(6)),
        (b"world", 8, None),
        (b"]", 1, None),
    ];
    for &(p, init, want) in cases {
        let ps = sref(&mut st, p);
        let w = want.map_or(1, |x| x as u64 + 2);
        assert_eq!(call(&mut st, lib::FIND, [s, ps, init as u64]), w, "plain find({p:?}, {init})");
        assert_eq!(call(&mut st, lib::FINDS, [s, ps, init as u64]), w, "find({p:?}, {init}) of a pattern without specials");
    }
    for p in [&b"l.o"[..], b"%", b"^h", b"a-b", b"(o)", b"o*", b"o+", b"o?", b"[o]", b"o$"] {
        let ps = sref(&mut st, p);
        assert_eq!(call(&mut st, lib::FINDS, [s, ps, 1]), 0, "find({p:?}): specials leave the trace");
        assert_ne!(call(&mut st, lib::FIND, [s, ps, 1]), 0, "plain find({p:?}) searches the bytes");
    }
    // a long subject and a long pattern (flag bits 0 and 3)
    let big: Vec<u8> = (0..300u32).map(|i| b'a' + (i % 26) as u8).collect();
    let bs = sref(&mut st, &big);
    let long_p = sref(&mut st, &big[100..160]);
    assert_eq!(call(&mut st, lib::FIND | 1 << 8 | 8 << 8, [bs, long_p, 1]), (100 % 26) as u64 + 2, "a long pattern is found at its first occurrence");
}

/// BUNDLE-C `tfmt`: the `FMT` helper runs the reference format engine: integers (`d x X c`, negative, extreme, widths, `-` and `0` flags), strings and
/// integers for `s`, the recorded result kind is compared (0 on a mismatch), the argument types come from the flags (2 bits each from bit 4), a
/// `__tostring` in the string metatable or a number metatable (for `%s` of an integer) answers 0, and an error the library would raise answers 0.
/// Detectors of the mutants `tfmt_kind_skip`, `tfmt_args_swap`.
#[test]
fn lib_fmt_follows_the_library() {
    let mut st = state();
    let reg = |f: &[u8]| rn_std::lib_string::jit_fmt_register(f).expect("registry") as u32;
    // (format, arg types: 0 int / 1 short / 2 long / 3 none, args, expected text)
    let big = vec![b'x'; 60];
    let bigs = sref(&mut st, &big);
    let ab = sref(&mut st, b"ab");
    let cases: Vec<(&[u8], [u32; 3], [u64; 3], Vec<u8>)> = vec![
        (b"%d", [0, 3, 3], [42, 0, 0], b"42".to_vec()),
        (b"%d", [0, 3, 3], [-7i64 as u64, 0, 0], b"-7".to_vec()),
        (b"%d", [0, 3, 3], [i64::MIN as u64, 0, 0], b"-9223372036854775808".to_vec()),
        (b"%x", [0, 3, 3], [255, 0, 0], b"ff".to_vec()),
        (b"%X", [0, 3, 3], [-1i64 as u64, 0, 0], b"FFFFFFFFFFFFFFFF".to_vec()),
        (b"%08x", [0, 3, 3], [48879, 0, 0], b"0000beef".to_vec()),
        (b"%5d|%-5d|", [0, 0, 3], [12, 34, 0], b"   12|34   |".to_vec()),
        (b"%c%c", [0, 0, 3], [72, 105, 0], b"Hi".to_vec()),
        (b"%3c|", [0, 3, 3], [65, 0, 0], b"  A|".to_vec()),
        (b"%%%d%%", [0, 3, 3], [5, 0, 0], b"%5%".to_vec()),
        (b"%s", [1, 3, 3], [ab, 0, 0], b"ab".to_vec()),
        (b"[%5s]", [1, 3, 3], [ab, 0, 0], b"[   ab]".to_vec()),
        (b"[%-5s]", [1, 3, 3], [ab, 0, 0], b"[ab   ]".to_vec()),
        (b"%s", [0, 3, 3], [123, 0, 0], b"123".to_vec()),
        (b"%s=%d", [1, 0, 3], [ab, 9, 0], b"ab=9".to_vec()),
        (b"%08x|%-6d|%5s", [0, 0, 1], [255, 7, ab], b"000000ff|7     |   ab".to_vec()),
    ];
    for (f, ty, args, want) in &cases {
        let k = lib::FMT_BASE + reg(f);
        let fl = (ty[0] << 4) | (ty[1] << 6) | (ty[2] << 8);
        let r = call(&mut st, k | fl << 8, *args);
        assert_ne!(r, 0, "format({:?}) did nothing", String::from_utf8_lossy(f));
        assert_eq!(text(r, false), *want, "format({:?})", String::from_utf8_lossy(f));
    }
    // the extended form: `FMTX` stashes the trailing arguments, the `FMT` of kind `FMT_BASE + 64 + id` takes two direct ones and the stash (its
    // third operand is the stash op's result, here the constant 1)
    let id = reg(b"%08x  %s  %-7s %s%s|");
    let cd = sref(&mut st, b"mov");
    let ef = sref(&mut st, b"r1, r2");
    let gh = sref(&mut st, b"x");
    assert_eq!(call(&mut st, lib::FMTX | ((1 << 4) | (1 << 6) | (1 << 8)) << 8, [cd, ef, gh]), 1);
    let r = call(&mut st, (lib::FMT_BASE + 64 + id) | ((0 << 4) | (1 << 6) | (0 << 8)) << 8, [77, ab, 1]);
    assert_ne!(r, 0, "extended format did nothing");
    assert_eq!(text(r, false), b"0000004d  ab  mov     r1, r2x|".to_vec());
    // the stash is consumed: a second extended call without a new stash does nothing
    assert_eq!(call(&mut st, (lib::FMT_BASE + 64 + id) | ((0 << 4) | (1 << 6) | (0 << 8)) << 8, [77, ab, 1]), 0, "no stash");
    // two stashed values (a four-argument format)
    let id4 = reg(b"%d-%s-%s-%d");
    assert_eq!(call(&mut st, lib::FMTX | ((1 << 4) | (0 << 6) | (3 << 8)) << 8, [cd, 9, 0]), 1);
    let r = call(&mut st, (lib::FMT_BASE + 64 + id4) | ((0 << 4) | (1 << 6) | (0 << 8)) << 8, [5, ab, 1]);
    assert_ne!(r, 0);
    assert_eq!(text(r, false), b"5-ab-mov-9".to_vec());
    // a long result: the recorded kind (flag bit 1) must match
    let f = b"%s-%s";
    let k = lib::FMT_BASE + reg(f);
    let fl = (1 << 4) | (1 << 6) | (3 << 8);
    assert_eq!(call(&mut st, k | fl << 8, [ab, ab, 0]), call(&mut st, k | fl << 8, [ab, ab, 0]), "short results are interned");
    assert_eq!(call(&mut st, k | (fl | 2) << 8, [ab, ab, 0]), 0, "a long result was recorded: the short one leaves the trace");
    let fl2 = (2 << 4) | (1 << 6) | (3 << 8);
    let r = call(&mut st, k | (fl2 | 2) << 8, [bigs, ab, 0]);
    assert_ne!(r, 0);
    assert_eq!(text(r, true), [&big[..], b"-ab"].concat());
    assert_eq!(call(&mut st, k | fl2 << 8, [bigs, ab, 0]), 0, "a long result was not recorded");
    // errors the library raises: a width over 99 digits, a string with a zero and a width
    let z = sref(&mut st, b"a\0b");
    let k = lib::FMT_BASE + reg(b"%5s");
    assert_eq!(call(&mut st, k | ((1 << 4) | (3 << 6) | (3 << 8)) << 8, [z, 0, 0]), 0, "string contains zeros");
    let k = lib::FMT_BASE + reg(b"%123d");
    assert_eq!(call(&mut st, k | ((3 << 6) | (3 << 8)) << 8, [1, 0, 0]), 0, "invalid conversion");
    // a `__tostring` in the string metatable: `%s` of a string leaves the trace; a number metatable: `%s` of an integer
    let k = lib::FMT_BASE + reg(b"%s");
    let fl = (1 << 4) | (3 << 6) | (3 << 8);
    assert_ne!(call(&mut st, k | fl << 8, [ab, 0, 0]), 0);
    let smt = st.type_metatable(LuaType::String).expect("string metatable");
    let key = st.new_str(b"__tostring").expect("key");
    let fval = Value::string(st.new_str(b"x").expect("string"));
    smt.set_str(st.heap(), key, &fval).expect("set");
    assert_eq!(call(&mut st, k | fl << 8, [ab, 0, 0]), 0, "__tostring in the string metatable");
    smt.set_str(st.heap(), key, &Value::NIL).expect("clear");
    assert_ne!(call(&mut st, k | fl << 8, [ab, 0, 0]), 0, "no __tostring any more");
}

/// BUNDLE-A `libnum`: the tonumber-mode conversion of `STRTO` (`strto_tonumber`): integer and float numerals, whitespace, hex (integer wraps, hex
/// float), `1e2`, an integer numeral past the integer range becomes a float, non-numerals give `None`. Detector of `tonum_mode_ignored` (the
/// floor-integer conversion of the `for` limit would not produce floats).
#[test]
fn strto_tonumber_follows_tonumber() {
    use rn_core::value::{LUA_VNUMFLT, LUA_VNUMINT};
    let mut st = state();
    let cases: &[(&[u8], Option<(u64, u8)>)] = &[
        (b"10", Some((10, LUA_VNUMINT))),
        (b" 12 ", Some((12, LUA_VNUMINT))),
        (b"\t-7\n", Some((-7i64 as u64, LUA_VNUMINT))),
        (b"0x1F", Some((31, LUA_VNUMINT))),
        (b"-0x10", Some((-16i64 as u64, LUA_VNUMINT))),
        (b"0xffffffffffffffff", Some((u64::MAX, LUA_VNUMINT))), // hex integers wrap
        (b"1e2", Some((100f64.to_bits(), LUA_VNUMFLT))),
        (b"3.5", Some((3.5f64.to_bits(), LUA_VNUMFLT))),
        (b".5", Some((0.5f64.to_bits(), LUA_VNUMFLT))),
        (b"5.", Some((5f64.to_bits(), LUA_VNUMFLT))),
        (b"0x.8p1", Some((1f64.to_bits(), LUA_VNUMFLT))),
        (b"9223372036854775807", Some((i64::MAX as u64, LUA_VNUMINT))),
        (b"9223372036854775808", Some(((i64::MAX as f64 + 1.0).to_bits(), LUA_VNUMFLT))),
        (b"-9223372036854775808", Some((i64::MIN as u64, LUA_VNUMINT))),
        (b"1e", None),
        (b"abc", None),
        (b"", None),
        (b"0x", None),
        (b"5 5", None),
        (b"inf", None),
        (b"nan", None),
        (b"1_0", None),
        (b"  ", None),
        (b"10\0", None),
    ];
    for &(s, want) in cases {
        let p = sref(&mut st, s);
        let v = unsafe { std::mem::transmute::<[u64; 2], Value>([u64::from(ctb(LUA_VSHRSTR)), p]) };
        assert_eq!(rn_jit::tr::asm::strto_tonumber(&v), want, "tonumber({:?})", String::from_utf8_lossy(s));
    }
    assert_eq!(rn_jit::tr::asm::strto_tonumber(&Value::int(5)), None);
}

/// Runs `src` (a chunk returning `nres` values) and returns the values.
fn run_lua(st: &mut State, src: &[u8], nres: usize) -> Vec<Value> {
    assert_eq!(st.load_buffer(src, b"=t", None), rn_core::status::Status::Ok);
    let f = st.top() - 1;
    assert_eq!(st.pcall(f, nres as i32, 0), rn_core::status::Status::Ok);
    (0..nres).map(|i| st.get(st.top() - nres + i)).collect()
}

/// BUNDLE-H: `string.char(a, b [, c])`: the bytes in order, above 255 did nothing. Detectors of the mutants `charn_range_skip`, `charn_order`.
#[test]
fn lib_char_n_makes_the_bytes_in_order() {
    let mut st = state();
    let r = call(&mut st, lib::CHAR2, [72, 105, 0]);
    assert_eq!(text(r, false), b"Hi");
    assert_eq!(r, sref(&mut st, b"Hi"));
    let r = call(&mut st, lib::CHAR3, [0, 255, 65]);
    assert_eq!(text(r, false), vec![0, 255, 65], "zero and high bytes");
    assert_eq!(call(&mut st, lib::CHAR2, [65, 256, 0]), 0);
    assert_eq!(call(&mut st, lib::CHAR2, [u64::MAX, 65, 0]), 0, "negative");
    assert_eq!(call(&mut st, lib::CHAR3, [65, 66, 256]), 0);
    assert_eq!(call(&mut st, lib::CHAR3, [65, u64::MAX - 5, 66]), 0);
}

/// BUNDLE-H: `math.random()`: the helper draws the library's sequence (the top 53 bits of each draw plus one), and does nothing for another closure.
/// Detectors of the mutant `randf_plus_zero`.
#[test]
fn lib_randf_follows_math_random() {
    let mut st = state();
    let rc = run_lua(&mut st, b"return math.random", 1)[0].as_cclosure().expect("closure").as_ptr() as u64;
    run_lua(&mut st, b"math.randomseed(42, 7)", 0);
    let got: Vec<u64> = (0..64).map(|_| call(&mut st, lib::RANDF, [rc, 0, 0])).collect();
    run_lua(&mut st, b"math.randomseed(42, 7)", 0);
    let want = run_lua(&mut st, b"local t = {} for i = 1, 64 do t[i] = math.random() end return table.unpack(t)", 64);
    for (g, w) in got.iter().zip(&want) {
        assert!(*g >= 1, "never 0 for a success");
        let f = (*g - 1) as f64 * (0.5 / (1u64 << 52) as f64);
        assert_eq!(f.to_bits(), w.as_float().expect("float").to_bits());
    }
    // another closure: nothing is drawn
    let it = run_lua(&mut st, b"return string.gmatch('a', 'a')", 1)[0];
    let cc = it.as_cclosure().expect("closure");
    assert_eq!(call(&mut st, lib::RANDF, [cc.as_ptr() as u64, 0, 0]), 0);
    run_lua(&mut st, b"math.randomseed(42, 7)", 0);
    assert_eq!(call(&mut st, lib::RANDF, [rc, 0, 0]), got[0], "the failed call changed nothing");
}

/// BUNDLE-H: `lower` / `upper` / `reverse` of short and long strings (embedded zeros, high bytes, the empty string). Detectors of the mutant `lower_is_upper`.
#[test]
fn lib_case_and_reverse_follow_the_library() {
    let mut st = state();
    let long: Vec<u8> = (0..70u32).map(|i| if i % 3 == 0 { b'a' + (i % 26) as u8 } else { b'A' + (i % 26) as u8 }).collect();
    let cases: Vec<Vec<u8>> = vec![b"".to_vec(), b"Hello World".to_vec(), vec![0, b'A', 0, b'z', 200, 255, b'Q'], b"already lower".to_vec(), long];
    for s in &cases {
        let sp = sref(&mut st, s);
        let is_long = s.len() > rn_core::string::MAX_SHORT_LEN;
        let fl = u32::from(is_long) | u32::from(is_long) << 1;
        let lower: Vec<u8> = s.iter().map(u8::to_ascii_lowercase).collect();
        let upper: Vec<u8> = s.iter().map(u8::to_ascii_uppercase).collect();
        let rev: Vec<u8> = s.iter().rev().copied().collect();
        for (k, want) in [(lib::LOWER, lower), (lib::UPPER, upper), (lib::REVERSE, rev)] {
            let r = call(&mut st, k | fl << 8, [sp, 0, 0]);
            assert_ne!(r, 0, "kind {k} of {} bytes", s.len());
            assert_eq!(text(r, is_long), want, "kind {k}");
        }
    }
}

/// BUNDLE-H: `string.rep(s, n [, sep])`: counts of every sign, separators, empty strings, the recorded result class, and the size cap (a count that would
/// exceed it, or overflow, does nothing). Nothing larger than the cap is allocated. Detectors of the mutants `rep_cap_skip`, `rep_sep_trailing`.
#[test]
fn lib_rep_follows_the_library() {
    let mut st = state();
    let ab = sref(&mut st, b"ab");
    let a = sref(&mut st, b"a");
    let comma = sref(&mut st, b",");
    let empty = sref(&mut st, b"");
    let rep = |st: &mut State, s: u64, n: i64, sep: Option<u64>, long: bool| -> u64 {
        let fl = u32::from(long) << 1;
        match sep {
            None => call(st, lib::REP | fl << 8, [s, n as u64, 0]),
            Some(p) => call(st, lib::REPS | fl << 8, [s, n as u64, p]),
        }
    };
    assert_eq!(text(rep(&mut st, ab, 3, None, false), false), b"ababab");
    assert_eq!(text(rep(&mut st, ab, 1, None, false), false), b"ab");
    assert_eq!(text(rep(&mut st, ab, 0, None, false), false), b"");
    assert_eq!(text(rep(&mut st, ab, -7, None, false), false), b"");
    assert_eq!(text(rep(&mut st, empty, 1000, None, false), false), b"");
    // BUNDLE-R: single-byte units (the buffer is filled by one resize), also across the class limit, with the separator cut at the end
    assert_eq!(text(rep(&mut st, a, 40, None, false), false), vec![b'a'; 40]);
    assert_eq!(text(rep(&mut st, a, 41, None, true), true), vec![b'a'; 41]);
    assert_eq!(text(rep(&mut st, empty, 42, Some(comma), true), true), vec![b','; 41]);
    assert_eq!(text(rep(&mut st, a, 1, Some(empty), false), false), b"a");
    assert_eq!(text(rep(&mut st, empty, 1, Some(comma), false), false), b"");
    let abc = sref(&mut st, b"abc");
    for n in 1..=60i64 {
        let want = b"abc".repeat(n as usize);
        assert_eq!(text(rep(&mut st, abc, n, None, want.len() > 40), want.len() > 40), want, "abc x {n}");
        let mut w2 = b"abc,".repeat(n as usize);
        w2.pop();
        assert_eq!(text(rep(&mut st, abc, n, Some(comma), w2.len() > 40), w2.len() > 40), w2, "abc,{n}");
    }
    assert_eq!(text(rep(&mut st, a, 3, Some(comma), false), false), b"a,a,a");
    assert_eq!(text(rep(&mut st, a, 1, Some(comma), false), false), b"a");
    assert_eq!(text(rep(&mut st, a, 0, Some(comma), false), false), b"");
    assert_eq!(text(rep(&mut st, empty, 3, Some(comma), false), false), b",,");
    assert_eq!(text(rep(&mut st, empty, i64::MAX, Some(empty), false), false), b"");
    // a long result
    let r = rep(&mut st, ab, 30, None, true);
    assert_eq!(text(r, true), b"ab".repeat(30));
    let r = rep(&mut st, a, 30, Some(comma), true);
    let mut want = b"a,".repeat(29);
    want.push(b'a');
    assert_eq!(text(r, true), want);
    // the recorded class is not the result's
    assert_eq!(rep(&mut st, ab, 30, None, false), 0);
    assert_eq!(rep(&mut st, ab, 3, None, true), 0);
    // above the cap, or an overflowing count: nothing done, nothing allocated
    let cap = lib::REP_MAX as i64;
    assert_eq!(rep(&mut st, a, cap + 1, None, true), 0);
    assert_eq!(rep(&mut st, ab, cap / 2 + 1, None, true), 0);
    assert_eq!(rep(&mut st, a, cap / 2 + 1, Some(comma), true), 0);
    assert_eq!(rep(&mut st, ab, i64::MAX, None, true), 0);
    assert_eq!(rep(&mut st, ab, 1 << 62, Some(comma), true), 0);
}
