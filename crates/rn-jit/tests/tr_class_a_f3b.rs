//! F3b: the string class-A helpers of TR1 (`SNEW` = `snew_core`, `CONCAT` = `concat_core` of `tr::asm`) in isolation, on a
//! real state.
//!
//! The contract the traces rely on is that of `tr_class_a.rs`: a helper that reports 0 has **done nothing** (no
//! allocation, the accounting unchanged) and does so exactly when the interpreter's `check_gc` right after the same
//! allocation would step the collector (or the allocation could fail, or it would grow the string table, or the case
//! is one the interpreter's general path owns: an empty operand, the other result kind than the recorded one);
//! otherwise it allocates with the interpreter's own accounting and returns the string the interpreter makes (the same
//! interned object for a short result). Detectors of the mutants `snew_no_gc_check`, `snew_bytes_small`,
//! `concat_no_gc_check`, `concat_bytes_small`, `concat_kind_skip`.
#![cfg(feature = "trace")]

use rn_core::number::Number;
use rn_core::state::{LuaState, State};
use rn_jit::tr::asm::{concat_core, snew_core};

fn state() -> LuaState {
    let mut st = rn_std::new_state_with_libs().expect("state");
    rn_jit::set_mode(&mut st, rn_jit::Mode::Off);
    st.heap().gc_set_stp(0);
    st.heap().gc_set_threshold(usize::MAX);
    st
}

fn sref(st: &mut State, s: &[u8]) -> u64 {
    st.new_str(s).expect("string").as_ptr() as u64
}

/// The number strings of a spread of integers and floats.
const NUMS: &[Number] = &[
    Number::Int(100), // (one-digit integers are the fixed one-byte strings of B3-6: they add no bytes)
    Number::Int(70),
    Number::Int(-123456789),
    Number::Int(i64::MIN),
    Number::Float(1.5),
    Number::Float(2.0),
    Number::Float(-0.0),
    Number::Float(1e100),
    Number::Float(0.1),
    Number::Float(f64::INFINITY),
    Number::Float(123456789012345.0),
];

/// `SNEW` allocates the interpreter's string with the interpreter's accounting; the threshold is crossed by exactly the
/// bytes the new string adds, an interned string adds none.
#[test]
fn snew_exits_exactly_when_a_step_would_be_due() {
    for &n in NUMS {
        let text = rn_core::number::tostring(n).as_bytes().to_vec();
        // the interpreter's conversion in a state of its own: the bytes it adds
        let mut ref_st = state();
        let before = ref_st.heap().total_bytes();
        let want = ref_st.new_str(&text).expect("string").as_ptr() as u64;
        let bytes = ref_st.heap().total_bytes() - before;
        let _ = want;
        assert!(bytes > 0);

        let mut st = state();
        let total = st.heap().total_bytes();
        for thr in [total + bytes - 1, total + bytes, total, 0] {
            st.heap().gc_set_threshold(thr);
            assert_eq!(snew_core(&mut st, n), 0, "{n:?} threshold {thr}: must exit");
            assert_eq!(st.heap().total_bytes(), total, "{n:?}: the exit allocated");
        }
        st.heap().gc_set_threshold(total + bytes + 1);
        let s = snew_core(&mut st, n);
        assert_ne!(s, 0, "{n:?}: must allocate");
        assert_eq!(st.heap().total_bytes(), total + bytes, "{n:?}: accounting");
        assert!(!st.heap().gc_due(), "{n:?}: the collector must stay idle");
        // the very object the interpreter's conversion finds again
        assert_eq!(sref(&mut st, &text), s, "{n:?}: interned");
        // an interned string adds nothing: only `total < threshold` matters
        let total = st.heap().total_bytes();
        st.heap().gc_set_threshold(total + 1);
        assert_eq!(snew_core(&mut st, n), s, "{n:?}: present string");
        st.heap().gc_set_threshold(total);
        assert_eq!(snew_core(&mut st, n), 0, "{n:?}: present string, threshold reached");
    }
}

/// The memory limit: an allocation that would fail is not attempted.
#[test]
fn snew_and_concat_exit_when_the_limit_would_fail() {
    let mut st = state();
    let a = sref(&mut st, b"limit-a");
    let b = sref(&mut st, b"limit-b");
    let total = st.heap().total_bytes();
    let n = Number::Int(987654321);
    let text = rn_core::number::tostring(n).as_bytes().to_vec();
    let bytes = rn_core::string::MAX_SHORT_LEN; // (only to have a figure: the exact size is read below)
    let _ = bytes;
    let mut ref_st = state();
    let t0 = ref_st.heap().total_bytes();
    ref_st.new_str(&text).unwrap();
    let need = ref_st.heap().total_bytes() - t0;
    st.heap().mem_mut().set_limit(total + need - 1);
    assert_eq!(snew_core(&mut st, n), 0);
    assert_eq!(st.heap().total_bytes(), total);
    st.heap().mem_mut().set_limit(total + need);
    assert_ne!(snew_core(&mut st, n), 0);
    st.heap().mem_mut().set_limit(usize::MAX);
    // CONCAT of two short strings: the same
    let total = st.heap().total_bytes();
    let mut ref_st = state();
    let (ra, rb) = (sref(&mut ref_st, b"limit-a"), sref(&mut ref_st, b"limit-b"));
    let _ = (ra, rb);
    let t0 = ref_st.heap().total_bytes();
    ref_st.new_str(b"limit-alimit-b").unwrap();
    let need = ref_st.heap().total_bytes() - t0;
    st.heap().mem_mut().set_limit(total + need - 1);
    // SAFETY: live short strings.
    assert_eq!(unsafe { concat_core(&mut st, 0, [a, b, 0]) }, 0);
    assert_eq!(st.heap().total_bytes(), total);
    st.heap().mem_mut().set_limit(total + need);
    // SAFETY: same call as above (valid state, operands built by this test).
    assert_ne!(unsafe { concat_core(&mut st, 0, [a, b, 0]) }, 0);
    st.heap().mem_mut().set_limit(usize::MAX);
}

/// `CONCAT` of two or three strings, short and long result: the exact bytes, the boundary, the interned result.
#[test]
fn concat_exits_exactly_when_a_step_would_be_due() {
    let shapes: &[(&[&[u8]], u32)] = &[
        (&[b"ab", b"cd"], 0),
        (&[b"x", b"yy", b"zzz"], 8),
        (&[&[b'q'; 20], &[b'r'; 20]], 0),      // 40 bytes: still short
        (&[&[b'q'; 20], &[b'r'; 21]], 16),     // 41 bytes: long
        (&[&[b's'; 100], b"t", &[b'u'; 300]], 8 | 16 | 1),
    ];
    for (parts, bits) in shapes {
        let total_len: usize = parts.iter().map(|p| p.len()).sum();
        let joined: Vec<u8> = parts.iter().flat_map(|p| p.iter().copied()).collect();
        let mut st = state();
        let mut ops = [0u64; 3];
        let mut bits = *bits;
        for (k, p) in parts.iter().enumerate() {
            ops[k] = sref(&mut st, p);
            if p.len() > rn_core::string::MAX_SHORT_LEN {
                bits |= 1 << k;
            }
        }
        // the interpreter's result in a state of its own: the bytes it adds
        let mut ref_st = state();
        for p in parts.iter() {
            sref(&mut ref_st, p);
        }
        let t0 = ref_st.heap().total_bytes();
        ref_st.new_str(&joined).unwrap();
        let bytes = ref_st.heap().total_bytes() - t0;
        let total = st.heap().total_bytes();
        for thr in [total + bytes - 1, total + bytes, total, 0] {
            st.heap().gc_set_threshold(thr);
            // SAFETY: live strings of the kinds `bits` says.
            assert_eq!(unsafe { concat_core(&mut st, bits, ops) }, 0, "{total_len} bytes, threshold {thr}: must exit");
            assert_eq!(st.heap().total_bytes(), total, "{total_len} bytes: the exit allocated");
        }
        st.heap().gc_set_threshold(total + bytes + 1);
        // SAFETY: as above.
        let r = unsafe { concat_core(&mut st, bits, ops) };
        assert_ne!(r, 0, "{total_len} bytes: must allocate");
        assert_eq!(st.heap().total_bytes(), total + bytes, "{total_len} bytes: accounting");
        // the string has the interpreter's contents; a short one is the interned object
        let v = st.new_str(&joined).unwrap().as_ptr() as u64;
        if total_len <= rn_core::string::MAX_SHORT_LEN {
            assert_eq!(r, v, "{total_len} bytes: interned result");
        }
        // SAFETY: the helper returned a live string.
        let s = unsafe {
            std::mem::transmute::<[u64; 2], rn_core::value::Value>([
                u64::from(if total_len <= rn_core::string::MAX_SHORT_LEN {
                    rn_core::value::ctb(rn_core::value::LUA_VSHRSTR)
                } else {
                    rn_core::value::ctb(rn_core::value::LUA_VLNGSTR)
                }),
                r,
            ])
        };
        assert_eq!(s.as_strref().unwrap().get().as_bytes(), &joined[..]);
        st.heap().gc_set_threshold(usize::MAX);
    }
}

/// The cases the interpreter's general path owns: an empty operand, and the other result kind than the recorded one.
#[test]
fn concat_leaves_special_cases_to_the_interpreter() {
    let mut st = state();
    let a = sref(&mut st, b"abc");
    let e = sref(&mut st, b"");
    let total = st.heap().total_bytes();
    // SAFETY: live short strings.
    unsafe {
        assert_eq!(concat_core(&mut st, 0, [a, e, 0]), 0, "empty second operand");
        assert_eq!(concat_core(&mut st, 0, [e, a, 0]), 0, "empty first operand");
        assert_eq!(concat_core(&mut st, 8, [a, a, e]), 0, "empty third operand");
        // recorded as a long result, but 6 bytes is short (and the reverse)
        assert_eq!(concat_core(&mut st, 16, [a, a, 0]), 0, "short result recorded long");
    }
    assert_eq!(st.heap().total_bytes(), total, "no special case allocated");
}

/// A stopped collector never steps: the helpers allocate whatever the threshold says.
#[test]
fn string_helpers_follow_stop() {
    let mut st = state();
    st.heap().gc_set_threshold(0);
    assert_eq!(snew_core(&mut st, Number::Int(424242)), 0, "threshold crossed: exits");
    st.heap().gc_set_stp(rn_core::gc::GCSTP_USER);
    assert_ne!(snew_core(&mut st, Number::Int(424242)), 0, "stopped: allocates");
}
