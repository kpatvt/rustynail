//! BUNDLE-D lowering tests: the tier-1 continuation after the Lua metamethod of an arithmetic instruction (`armm`), seen in the resume table.
#![cfg(feature = "trace")]

use rn_core::instruction::Instruction;
use rn_core::state::LuaState;
use rn_core::status::Status;
use rn_core::ver::active::OpCode;
use rn_jit::lower::calls::CallEnv;
use rn_jit::lower::{lower, LowerCfg};

fn env(armm: bool) -> CallEnv {
    CallEnv {
        stack_off: 0,
        proto_code: 0,
        proto_k: 0,
        call_helper: 0,
        ccall_helper: 0,
        armm,
        tail_helper: 0,
        tail_fast: false,
        tail_mut: 0,
        idx_name: 0,
        idx_hash: 0,
        idx_levels: 0,
        str_mt: 0,
        inl: 0,
        inl_mut: 0,
        t1q: 0,
        hints: std::ptr::null(),
        tset: 0,
    }
}

/// The resume pcs of the main function of `src` lowered with `armm`, and the opcode of the instruction before each.
fn resume_before(src: &str, armm: bool) -> Vec<(u32, Option<OpCode>)> {
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    assert_eq!(st.load_buffer(src.as_bytes(), b"=c", None), Status::Ok);
    let cl = st.get(st.top() - 1).as_lclosure().expect("closure");
    let p = cl.proto();
    let cfg = LowerCfg { code_addr: p.code_ptr() as usize, interrupt_addr: 0, consts: p.k(), t15_warmup: None, calls: Some(env(armm)) };
    let low = lower(p.code(), cfg).expect("lowered");
    let mut v: Vec<(u32, Option<OpCode>)> = low.resume.iter().filter(|&&(pc, _)| pc > 0).map(|&(pc, _)| (pc, Instruction(p.code()[pc as usize - 1]).opcode())).collect();
    v.sort_by_key(|x| x.0);
    v
}

const SRC: &str = "local a, b = ... local t = a + b local u = -a local w = a - 2 local x = t * u + w return t + u + w + x";

/// With `armm` the instruction after every `MMBIN*` / `UNM` of the function is a resume pc (the continuation of the returning metamethod); without it
/// there is none. Detector of the mutant `armm_never`.
#[test]
fn bd_armm_lowering() {
    let on = resume_before(SRC, true);
    let off = resume_before(SRC, false);
    let is_mm = |o: Option<OpCode>| matches!(o, Some(OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink | OpCode::Unm | OpCode::Bnot));
    let n_on = on.iter().filter(|(_, o)| is_mm(*o)).count();
    let n_off = off.iter().filter(|(_, o)| is_mm(*o)).count();
    assert_eq!(n_off, 0, "no continuation without the limit: {off:?}");
    // add, unm, subtract-constant, mul, add, add, add: the first six instructions that can call a metamethod each have one
    assert!(n_on >= 6, "continuations after the arithmetic instructions: {on:?}");
}

/// BUNDLE-D `sbump2`: the extra unroll grows with the failed attempts of the exit, is bounded by the knob, applies only inside inlined frames and to the
/// recursion-limit aborts; a repeated abort of that kind is not a "repeat" (the blacklisting rule) while the escalation has attempts left.
/// Detector of the mutants `sbump2_never` and `sbump2_unbounded`.
#[test]
fn bd_sbump2_rule() {
    use rn_jit::tr::cache::{sbump2_extra, sbump2_pending};
    assert_eq!(sbump2_extra(0, Some("Depth"), 3, true), 0, "off");
    assert_eq!(sbump2_extra(2, Some("Depth"), 0, true), 0, "first attempt");
    assert_eq!(sbump2_extra(2, Some("Depth"), 1, true), 1);
    assert_eq!(sbump2_extra(2, Some("Recursion"), 2, true), 2);
    assert_eq!(sbump2_extra(2, Some("SideShort"), 4, true), 2, "bounded by the knob");
    assert_eq!(sbump2_extra(2, Some("Depth"), 3, false), 0, "not inside inlined frames");
    assert_eq!(sbump2_extra(2, Some("Nyi(Loadk)"), 3, true), 0, "other aborts");
    assert_eq!(sbump2_extra(2, None, 3, true), 0);
    assert!(sbump2_pending(2, "Depth", 1) && sbump2_pending(2, "Recursion", 2));
    assert!(!sbump2_pending(2, "Depth", 3), "out of attempts: a repeat blacklists");
    assert!(!sbump2_pending(0, "Depth", 1) && !sbump2_pending(2, "FnShort", 1));
}
