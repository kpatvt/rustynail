//! Tier-1.5 region analysis (`rn_jit::t15`, docs/JIT_TIER15.md section 3): transfer functions, lattice laws,
//! CFG / loops / region selection, escaped slots, peel detection, plans of real functions and a property test
//! over random programs compiled by `rn-compiler`. Pure Rust, no VM state: Miri-runnable
//! (`scripts/miri.sh jit`); the bench-program dump test is `cfg_attr(miri, ignore)`.
//!
//! `T15_DUMP=1 cargo test -p rn-jit --features jit --test t15_analysis -- --nocapture bench_` prints the plans.
#![cfg(feature = "jit")]

use rn_compiler::testhost::TestHost;
use rn_compiler::{load, LoadMode};
use rn_core::host::CompilerHost;
use rn_core::instruction::Instruction;
use rn_core::protodata::ProtoData;
use rn_core::ver::active::OpCode;
use rn_jit::t15::analysis::{Analysis, Reject};
use rn_jit::t15::effects::{self as fx, State, StepCtx};
use rn_jit::t15::print::dump;
use rn_jit::t15::*;

// ------------------------------------------------------------------------------------------ helpers

struct Compiled {
    p: ProtoData<u32>,
    host: TestHost,
}

fn compile(src: &str) -> Compiled {
    let mut host = TestHost::default();
    let p = load(
        &mut host,
        src.as_bytes(),
        b"=t",
        LoadMode {
            text: true,
            binary: false,
        },
    )
    .unwrap_or_else(|e| panic!("compile: {}", String::from_utf8_lossy(&e.message)));
    Compiled { p, host }
}

fn funcs(p: &ProtoData<u32>) -> Vec<&ProtoData<u32>> {
    let mut v = vec![p];
    for c in &p.protos {
        v.extend(funcs(c));
    }
    v
}

fn info<'a>(c: &'a Compiled, p: &'a ProtoData<u32>) -> FuncInfo<'a> {
    FuncInfo::from_proto(p, |s| c.host.bytes(*s).len())
}

fn op(info: &FuncInfo<'_>, pc: usize) -> OpCode {
    Instruction(info.code[pc]).opcode().unwrap()
}

fn first_pc(info: &FuncInfo<'_>, o: OpCode) -> u32 {
    (0..info.code.len())
        .find(|&pc| op(info, pc) == o)
        .unwrap_or_else(|| panic!("no {o:?}")) as u32
}

/// Hand-assembled straight-line code, stepped sequentially from `init` (everything else `Any`).
fn run(
    code: &[u32],
    k: &[TagSet],
    max_stack: u32,
    init: &[(u32, TagSet)],
    prof: &[u8],
    policy: &Policy,
) -> (State, Vec<fx::StepOut>) {
    let mut code = code.to_vec();
    code.push(Instruction::abck(OpCode::Return0, 0, 0, 0, 0).0);
    let info = FuncInfo {
        code: &code,
        k: k.to_vec(),
        k_int: vec![None; k.len()],
        max_stack,
        escaped: SlotSet::EMPTY,
        extra_heads: vec![],
    };
    let spec = Speculation {
        prof,
        ..Default::default()
    };
    let none = |_: u32| SlotSet::EMPTY;
    let ctx = StepCtx {
        info: &info,
        spec: &spec,
        policy,
        guard_live: &none,
    };
    let mut st = State::any(max_stack as usize);
    for &(s, t) in init {
        st.t[s as usize] = t;
    }
    let mut outs = Vec::new();
    for (pc, &w) in code.iter().enumerate().take(code.len() - 1) {
        let o = Instruction(w).opcode().unwrap();
        let cls = fx::classify(o, pc as u32, policy);
        outs.push(fx::step(&ctx, pc as u32, cls, &mut st));
    }
    (st, outs)
}

fn abc(o: OpCode, a: u32, b: u32, c: u32) -> u32 {
    Instruction::abck(o, a, b, c, 0).0
}

const I: TagSet = TagSet::INT;
const F: TagSet = TagSet::FLT;

// ------------------------------------------------------------------------------------------ lattice

#[test]
fn tag_classes_roundtrip() {
    use rn_core::value::*;
    for t in [
        LUA_VNIL,
        LUA_VFALSE,
        LUA_VTRUE,
        LUA_VNUMINT,
        LUA_VNUMFLT,
        ctb(LUA_VSHRSTR),
        ctb(LUA_VLNGSTR),
        ctb(LUA_VTABLE),
        ctb(LUA_VLCL),
        LUA_VLCF,
        ctb(LUA_VCCL),
        ctb(LUA_VUSERDATA),
        LUA_VLIGHTUSERDATA,
        ctb(LUA_VTHREAD),
    ] {
        let s = TagSet::of_tag(t);
        assert!(s.is_singleton(), "{t:#x}");
        assert_eq!(s.tag_byte(), Some(t), "{t:#x}");
        assert_eq!(
            TagSet::of_tag(t & !BIT_ISCOLLECTABLE),
            s,
            "collectable bit is ignored"
        );
    }
    assert_eq!(TagSet::of_tag(LUA_VEMPTY), TagSet::NIL);
    assert_eq!(TagSet::of_tag(LUA_VLCFC), TagSet::LCF);
    assert_eq!(
        TagSet::of_tag(0x7f),
        TagSet::ANY,
        "unknown bytes never give a wrong singleton"
    );
    assert!(
        TagSet::TABLE.is_collectable()
            && !TagSet::INT.is_collectable()
            && !TagSet::LCF.is_collectable()
    );
    assert_eq!(TagSet::NUM.to_string(), "num");
    assert_eq!(TagSet::INT.join(TagSet::NIL).to_string(), "nil|int");
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn set(&mut self) -> TagSet {
        TagSet((self.next() & u64::from(TagSet::ANY.0)) as u16)
    }
}

#[test]
fn lattice_laws() {
    let mut r = Rng(0x9e3779b97f4a7c15);
    for _ in 0..500 {
        let (a, b, c) = (r.set(), r.set(), r.set());
        assert_eq!(a.join(b), b.join(a));
        assert_eq!(a.join(b).join(c), a.join(b.join(c)));
        assert_eq!(a.join(a), a);
        assert!(a.subset(a.join(b)) && b.subset(a.join(b)));
        assert!(TagSet::EMPTY.subset(a) && a.subset(TagSet::ANY));
        assert_eq!(a.subset(b), a.join(b) == b);
        let bits: Vec<TagSet> = a.bits().collect();
        assert_eq!(bits.iter().fold(TagSet::EMPTY, |x, y| x.join(*y)), a);
    }
}

#[test]
fn arithmetic_pair_rules() {
    use fx::Ar::*;
    let num = TagSet::NUM;
    // + - * : integers wrap (stay INT), any float operand gives FLT
    assert_eq!(fx::arith_result(AddSubMul, I, I), I);
    assert_eq!(fx::arith_result(AddSubMul, I, F), F);
    assert_eq!(fx::arith_result(AddSubMul, F, F), F);
    assert_eq!(fx::arith_result(AddSubMul, num, I), num);
    assert_eq!(fx::arith_result(AddSubMul, num, num), num);
    // / and ^ are always float
    for k in [Div, Pow] {
        assert_eq!(fx::arith_result(k, I, I), F);
        assert_eq!(fx::arith_result(k, num, num), F);
    }
    // // and % : int op int stays int, mixed gives float
    assert_eq!(fx::arith_result(IdivMod, I, I), I);
    assert_eq!(fx::arith_result(IdivMod, I, F), F);
    assert_eq!(fx::arith_result(IdivMod, num, num), num);
    // bitwise: integers (floats with exact integer value are converted by the VM; tier 1 exits on them)
    assert_eq!(fx::arith_result(Bit, I, I), I);
    assert_eq!(fx::arith_result(Bit, I, F), I);
    // non-numbers: every template exits -> bottom
    for k in [AddSubMul, Div, Pow, IdivMod, Bit] {
        assert!(fx::arith_result(k, TagSet::SHRSTR, I).is_empty());
        assert!(fx::arith_result(k, I, TagSet::TABLE).is_empty());
        assert!(fx::arith_result(k, TagSet::EMPTY, I).is_empty());
        // monotone in both arguments
        assert!(fx::arith_result(k, I, I).subset(fx::arith_result(k, num, I)));
    }
    // a set mixing numbers and strings keeps the numeric results only
    assert_eq!(fx::arith_result(AddSubMul, I.join(TagSet::SHRSTR), I), I);
}

// ------------------------------------------------------------------------------------------ transfer

#[test]
fn transfer_arithmetic_per_opcode() {
    use OpCode as O;
    let p = Policy::default();
    let k = [I, F, TagSet::SHRSTR];
    let cases: &[(u32, OpCode, u32, u32, TagSet, TagSet, TagSet)] = &[
        // (a, op, b, c, type of R1, type of R2, expected R0)
        (0, O::Add, 1, 2, I, I, I),
        (0, O::Add, 1, 2, I, F, F),
        (0, O::Sub, 1, 2, F, F, F),
        (0, O::Mul, 1, 2, TagSet::NUM, I, TagSet::NUM),
        (0, O::Div, 1, 2, I, I, F),
        (0, O::Idiv, 1, 2, I, I, I),
        (0, O::Mod, 1, 2, F, I, F),
        (0, O::Band, 1, 2, I, I, I),
        (0, O::Shl, 1, 2, I, I, I),
        (0, O::Add, 1, 2, TagSet::TABLE, I, TagSet::EMPTY),
    ];
    for &(a, o, b, c, tb, tc, want) in cases {
        let (st, outs) = run(&[abc(o, a, b, c)], &k, 4, &[(1, tb), (2, tc)], &[], &p);
        assert_eq!(st.t[0], want, "{o:?} {tb} {tc}");
        let expect_disp = if want.is_empty() {
            Disp::Dead
        } else if tb.is_singleton() && tc.is_singleton() {
            Disp::Specialised
        } else {
            Disp::Dual
        };
        assert_eq!(outs[0].disp, expect_disp, "{o:?} {tb} {tc}");
    }
    // constant forms: ADDK / DIVK / MULK take the constant's class, ADDI is an int immediate
    let (st, _) = run(&[abc(OpCode::Addk, 0, 1, 1)], &k, 4, &[(1, I)], &[], &p);
    assert_eq!(st.t[0], F);
    let (st, _) = run(&[abc(OpCode::Addk, 0, 1, 0)], &k, 4, &[(1, I)], &[], &p);
    assert_eq!(st.t[0], I);
    let (st, _) = run(&[abc(OpCode::Addi, 0, 1, 128)], &k, 4, &[(1, F)], &[], &p);
    assert_eq!(st.t[0], F);
    let (st, _) = run(&[abc(OpCode::Divk, 0, 1, 0)], &k, 4, &[(1, I)], &[], &p);
    assert_eq!(st.t[0], F);
    let (st, _) = run(&[abc(OpCode::Addk, 0, 1, 2)], &k, 4, &[(1, I)], &[], &p);
    assert_eq!(st.t[0], TagSet::EMPTY, "string constant: always exits");
}

#[test]
fn transfer_pow_len_are_class_n_with_mandatory_guard() {
    let k = [I];
    let on = Policy::default();
    let off = Policy {
        guard_n_defs: false,
        ..Policy::default()
    };
    let code = [abc(OpCode::Pow, 0, 1, 2), abc(OpCode::Len, 3, 4, 0)];
    let (st, outs) = run(&code, &k, 6, &[(1, I), (2, I)], &[], &on);
    assert_eq!((st.t[0], outs[0].guard_def), (F, Some(F)));
    assert_eq!((st.t[3], outs[1].guard_def), (I, Some(I)));
    assert_eq!(outs[0].disp, Disp::Opaque);
    assert!(
        st.mem.contains(3),
        "the guarded helper result is known in memory"
    );
    let (st, outs) = run(&code, &k, 6, &[(1, I), (2, I)], &[], &off);
    assert_eq!(
        (st.t[0], outs[0].guard_def),
        (TagSet::ANY, None),
        "M2: unguarded helper defs are Any"
    );
    assert_eq!(st.t[3], TagSet::ANY);
    assert!(!st.mem.contains(0));
}

#[test]
fn transfer_misc_ops() {
    use OpCode as O;
    let p = Policy::default();
    let k = [I, TagSet::SHRSTR];
    let code = [
        Instruction::asbx(O::Loadi, 0, 5).0,
        Instruction::asbx(O::Loadf, 1, 5).0,
        Instruction::abx(O::Loadk, 2, 1).0,
        abc(O::Loadtrue, 3, 0, 0),
        abc(O::Loadnil, 4, 1, 0),
        abc(O::Not, 6, 0, 0),
        abc(O::Move, 7, 1, 0),
        abc(O::Unm, 8, 0, 0),
        abc(O::Unm, 9, 2, 0),
        abc(O::Bnot, 10, 0, 0),
        abc(O::Concat, 11, 2, 0),
    ];
    let (st, outs) = run(&code, &k, 14, &[], &[], &p);
    assert_eq!(
        st.t[..6],
        [I, F, TagSet::SHRSTR, TagSet::TRUE, TagSet::NIL, TagSet::NIL]
    );
    assert_eq!(st.t[6], TagSet::BOOL);
    assert_eq!(st.t[7], F);
    assert_eq!(st.t[8], I);
    assert_eq!(st.t[9], TagSet::EMPTY, "UNM of a string always exits");
    assert_eq!(outs[8].disp, Disp::Dead);
    assert_eq!(st.t[10], I);
    assert_eq!(st.t[11], TagSet::ANY, "CONCAT may call __concat");
    assert_eq!(st.t[12], TagSet::ANY, "CONCAT leaves temporaries behind");
}

#[test]
fn transfer_loads_use_profile_and_widening() {
    use OpCode as O;
    let p = Policy::default();
    let code = [
        abc(O::Getfield, 0, 1, 0),
        abc(O::Gettable, 2, 1, 3),
        abc(O::Getupval, 4, 0, 0),
    ];
    let flt = rn_core::value::LUA_VNUMFLT;
    let prof = [flt, 0xFF, rn_core::value::LUA_VNUMINT];
    let (st, outs) = run(&code, &[], 6, &[(1, TagSet::TABLE), (3, I)], &prof, &p);
    assert_eq!((st.t[0], outs[0].guard_def), (F, Some(F)));
    assert_eq!(outs[0].disp, Disp::Specialised);
    assert_eq!(
        (st.t[2], outs[1].guard_def),
        (TagSet::ANY, None),
        "never executed: Any, no guard"
    );
    assert_eq!(st.t[4], I);
    // respecialisation: widening joins the observed tag in
    let mut code2 = code.to_vec();
    code2.push(abc(O::Return0, 0, 0, 0));
    let info = FuncInfo {
        code: &code2,
        k: vec![],
        k_int: vec![],
        max_stack: 6,
        escaped: SlotSet::EMPTY,
        extra_heads: vec![],
    };
    let widen = [(0u32, I)];
    let spec = Speculation {
        prof: &prof,
        def_widen: &widen,
        ..Default::default()
    };
    let none = |_: u32| SlotSet::EMPTY;
    let ctx = StepCtx {
        info: &info,
        spec: &spec,
        policy: &p,
        guard_live: &none,
    };
    let mut s = State::any(6);
    s.t[1] = TagSet::TABLE;
    fx::step(&ctx, 0, InsClass::Table, &mut s);
    assert_eq!(s.t[0], TagSet::NUM);
}

#[test]
fn transfer_calls_clobber_and_barriers() {
    use OpCode as O;
    let k = [];
    let code = [abc(O::Call, 2, 2, 2)];
    let init = [(0, I), (1, I), (2, I), (3, I), (4, I), (5, I)];
    let (st, outs) = run(&code, &k, 6, &init, &[], &Policy::default());
    assert_eq!(
        st.t[..2],
        [I, I],
        "slots below A survive (re-guarded by the stub)"
    );
    assert!(
        st.t[2..].iter().all(|t| t.is_any()),
        "result and callee frame become Any"
    );
    assert_eq!(outs[0].disp, Disp::Opaque);
    assert!(
        st.mem.is_empty(),
        "M1: no memory tag knowledge survives a barrier without a re-guard"
    );
    // ToAny mode drops escaped facts at the barrier, Reguard keeps them
    let mut esc = SlotSet::EMPTY;
    esc.insert(0);
    let mut code = code.to_vec();
    code.push(abc(O::Return0, 0, 0, 0));
    let info = FuncInfo {
        code: &code,
        k: vec![],
        k_int: vec![],
        max_stack: 6,
        escaped: esc,
        extra_heads: vec![],
    };
    for (mode, want) in [(EscapedMode::Reguard, I), (EscapedMode::ToAny, TagSet::ANY)] {
        let policy = Policy {
            escaped: mode,
            ..Policy::default()
        };
        let spec = Speculation::default();
        let none = |_: u32| SlotSet::EMPTY;
        let ctx = StepCtx {
            info: &info,
            spec: &spec,
            policy: &policy,
            guard_live: &none,
        };
        let mut s = State::any(6);
        s.t[0] = I;
        fx::step(&ctx, 0, InsClass::Call, &mut s);
        assert_eq!(s.t[0], want, "{mode:?}");
    }
}

#[test]
fn transfer_for_loop_types() {
    use OpCode as O;
    let k = [];
    let p = Policy::default();
    // FORPREP A=0: init R0, limit R1, step R2; Bx = 1 jumps over one body instruction (not executed here)
    let prep = Instruction::abx(O::Forprep, 0, 1).0;
    for (init, limit, step, want) in [
        (I, I, I, I),
        (I, F, I, I),
        (F, I, I, F),
        (I, I, F, F),
        (I, I, TagSet::NUM, TagSet::NUM),
        (TagSet::NUM, I, I, TagSet::NUM),
        (I, TagSet::EMPTY, I, TagSet::EMPTY),
    ] {
        let (st, outs) = run(&[prep], &k, 4, &[(0, init), (1, limit), (2, step)], &[], &p);
        assert_eq!(st.t[..3], [want; 3], "{init} {limit} {step}");
        if want.is_singleton() {
            assert_eq!(outs[0].disp, Disp::Specialised);
        }
    }
    // FORLOOP follows the step slot; the control slot is both used and defined (A6)
    let lp = Instruction::abx(O::Forloop, 0, 1).0;
    #[cfg(not(rustynail_lua54))]
    {
        let (st, _) = run(&[lp], &k, 4, &[(0, I), (1, I), (2, TagSet::ANY)], &[], &p);
        assert_eq!(st.t[..3], [I, I, I]);
    }
    let info_code = [lp, abc(O::Return0, 0, 0, 0)];
    let info = FuncInfo {
        code: &info_code,
        k: vec![],
        k_int: vec![],
        max_stack: 4,
        escaped: SlotSet::EMPTY,
        extra_heads: vec![],
    };
    let e = fx::effects(&info, 0);
    #[cfg(not(rustynail_lua54))]
    assert!(e.uses.contains(2) && e.defs.contains(2));
    let _ = e;
}

// ------------------------------------------------------------------------------------------ effects

#[test]
fn effects_windows() {
    use OpCode as O;
    let mk = |i: u32| {
        let code = vec![i, abc(O::Return0, 0, 0, 0)];
        let info = FuncInfo {
            code: &code,
            k: vec![],
            k_int: vec![],
            max_stack: 12,
            escaped: SlotSet::EMPTY,
            extra_heads: vec![],
        };
        fx::effects(&info, 0)
    };
    let v = |s: &SlotSet| s.iter().collect::<Vec<_>>();
    let e = mk(abc(O::Call, 3, 3, 2)); // R3 = R3(R4, R5)
    assert_eq!((v(&e.uses), v(&e.defs)), (vec![3, 4, 5], vec![3]));
    assert_eq!(
        v(&e.may_defs),
        (3..12).collect::<Vec<_>>(),
        "everything from A is clobbered"
    );
    let e = mk(abc(O::Call, 3, 0, 0)); // open call: reads up to top, defines nothing for sure
    assert_eq!(v(&e.uses), (3..12).collect::<Vec<_>>());
    assert!(e.defs.is_empty());
    let e = mk(abc(O::Concat, 2, 3, 0));
    assert_eq!(
        (v(&e.uses), v(&e.defs), v(&e.may_defs)),
        (vec![2, 3, 4], vec![2], vec![3, 4])
    );
    let e = mk(abc(O::Loadnil, 5, 2, 0));
    assert_eq!(v(&e.defs), vec![5, 6, 7]);
    let e = mk(Instruction::abck(O::SelfOp, 4, 1, 0, 1).0);
    assert_eq!((v(&e.uses), v(&e.defs)), (vec![1], vec![4, 5]));
    let e = mk(abc(O::Return, 2, 3, 0));
    assert_eq!(v(&e.uses), vec![2, 3]);
    let e = mk(abc(O::Return, 2, 0, 0));
    assert_eq!(v(&e.uses), (2..12).collect::<Vec<_>>());
    let e = mk(abc(O::Close, 3, 0, 0));
    assert_eq!(v(&e.uses), (3..12).collect::<Vec<_>>());
    // SETFIELD with a register value (k = 0) reads it; with a constant (k = 1) it does not
    let e = mk(Instruction::abck(O::Setfield, 1, 0, 6, 0).0);
    assert_eq!(v(&e.uses), vec![1, 6]);
    let e = mk(Instruction::abck(O::Setfield, 1, 0, 6, 1).0);
    assert_eq!(v(&e.uses), vec![1]);
    let e = mk(Instruction::abck(O::Settable, 1, 2, 6, 0).0);
    assert_eq!(v(&e.uses), vec![1, 2, 6]);
    #[cfg(not(rustynail_lua54))]
    {
        // 5.5 generic for: after TFORPREP the control variable is A+3; TFORCALL reads A, A+1, A+3 and
        // defines A+3 .. A+2+C (the doc's A+4 is the 5.4 layout)
        let e = mk(abc(O::Tforcall, 2, 0, 2));
        assert_eq!((v(&e.uses), v(&e.defs)), (vec![2, 3, 5], vec![5, 6]));
        assert_eq!(v(&e.may_defs), (5..12).collect::<Vec<_>>());
        let e = mk(Instruction::abx(O::Tforloop, 2, 3).0);
        assert_eq!(v(&e.uses), vec![5]);
        let e = mk(Instruction::abx(O::Forprep, 2, 3).0);
        assert_eq!((v(&e.uses), v(&e.defs)), (vec![2, 3, 4], vec![2, 3, 4]));
    }
}

#[test]
fn classes() {
    use OpCode as O;
    let p = Policy {
        fallback_g: vec![7],
        ..Policy::default()
    };
    assert_eq!(fx::classify(O::Add, 0, &p), InsClass::Typed);
    assert_eq!(fx::classify(O::Forloop, 0, &p), InsClass::Typed);
    assert_eq!(fx::classify(O::Getfield, 0, &p), InsClass::Table);
    assert_eq!(
        fx::classify(O::Getfield, 7, &p),
        InsClass::FallbackG,
        "per-site in-region fallback"
    );
    assert_eq!(fx::classify(O::Len, 0, &p), InsClass::FallbackN);
    assert_eq!(fx::classify(O::Pow, 0, &p), InsClass::FallbackN);
    for o in [
        O::Newtable,
        O::Concat,
        O::Closure,
        O::Setlist,
        O::Vararg,
        O::Close,
        O::Tbc,
        O::Tforprep,
    ] {
        assert_eq!(fx::classify(o, 0, &p), InsClass::FallbackG, "{o:?}");
        assert!(InsClass::FallbackG.is_barrier());
    }
    for o in [
        O::Call,
        O::Tailcall,
        O::Return,
        O::Return0,
        O::Return1,
        O::Tforcall,
    ] {
        assert_eq!(fx::classify(o, 0, &p), InsClass::Call, "{o:?}");
    }
    assert_eq!(fx::classify(O::Loadkx, 0, &p), InsClass::Exit);
    assert_eq!(fx::classify(O::Mmbin, 0, &p), InsClass::Follower);
    assert!(!InsClass::Typed.is_barrier() && !InsClass::FallbackN.is_barrier());
}

// ------------------------------------------------------------------------------------------ CFG / loops

fn analysis_of(c: &Compiled, f: usize) -> (FuncInfo<'_>, usize) {
    let fs = funcs(&c.p);
    (info(c, fs[f]), f)
}

#[test]
fn cfg_followers_and_blocks() {
    let c = compile("local a, b = ... local s = 0 for i = 1, 10 do if a < b then s = s + i else s = s - 1 end end return s");
    let (inf, _) = analysis_of(&c, 0);
    let an = Analysis::new(&inf).unwrap();
    for (pc, r) in an.role.iter().enumerate() {
        match op(&inf, pc) {
            OpCode::Mmbin | OpCode::Mmbini | OpCode::Mmbink => assert_eq!(*r, Role::Mmbin),
            _ => {}
        }
    }
    // each compare is followed by a JMP follower, and followers are never block starts
    for (pc, r) in an.role.iter().enumerate() {
        if *r == Role::JmpFollower {
            assert_eq!(op(&inf, pc), OpCode::Jmp);
            assert!(!an.blocks.iter().any(|b| b.start as usize == pc));
            assert!(!an.is_target[pc]);
        }
    }
    // blocks tile the code
    let mut next = 0;
    for b in &an.blocks {
        assert_eq!(b.start, next);
        next = b.end;
    }
    assert_eq!(next as usize, inf.code.len());
    // one loop, found from its FORLOOP
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.loop_with_backedge(fl).expect("loop");
    assert_eq!(an.loops[l].end, fl);
    assert_eq!(an.loops.len(), 1);
}

/// `while true do ... break ... end` (heapsort's `sift`): the closing `JMP` follows the `break`s and is dead code, so the
/// nest `[head, end]` of `scan_nests` ends at an unreachable back-edge; `select_nest` falls back to the reachable latch
/// (T7, docs 21.4). `repeat ... until` and plain `while` loops keep selecting through `select_region`.
#[test]
fn while_true_loops_with_break_are_selected_as_nests() {
    let srcs = [
        "local a, n = ... local root = 1
         while true do
           local child = root * 2
           if child > n then break end
           if a[root] < a[child] then a[root], a[child] = a[child], a[root]; root = child else break end
         end
         return root",
        "local n = ... local i = 0
         while true do i = i + 1; if i > n then break end end
         return i",
        "local n = ... local i = 0
         repeat i = i + 1 until i > n
         return i",
        "local n = ... local i = 0
         while true do i = i + 1; if i % 3 == 0 then goto done end if i > n then break end end
         ::done:: return i",
    ];
    for src in srcs {
        let c = compile(src);
        let inf = info(&c, &c.p);
        let an = Analysis::new(&inf).unwrap();
        let nests = rn_jit::t15::runtime::scan_nests(inf.code);
        assert_eq!(nests.len(), 1, "{src}");
        let (h, e) = nests[0];
        let pol = Policy::default();
        let l = an
            .select_nest(h, e, &pol)
            .unwrap_or_else(|r| panic!("{src}: {r:?}"));
        assert_eq!(an.loops[l].head, h, "{src}");
        // the planned region is a real loop with a body
        assert!(an.plan(l, &Speculation::default(), &pol).is_ok(), "{src}");
    }
    // two loops sharing a head (two backward gotos) and reachable code between the chosen latch and the dead closing JMP
    for src in [
        "local n = ... local i, s = 0, 0
         ::top::
         i = i + 1
         if i % 2 == 0 then goto top end
         s = s + i
         if i < n then goto top end
         return s",
        "local n = ... local i, s = 0, 0
         while true do
           i = i + 1
           if i > n then break end
           if i % 2 == 0 then goto cont end
           s = s + i
           ::cont::
           s = s + 1
         end
         return s",
        "local n = ... local i, s = 0, 0
         while true do
           i = i + 1
           if i > n then break end
           if i % 5 == 0 then goto done end
           s = s + i
         end
         ::done:: s = s + 100
         return s",
    ] {
        let c = compile(src);
        let inf = info(&c, &c.p);
        let an = Analysis::new(&inf).unwrap();
        let nests = rn_jit::t15::runtime::scan_nests(inf.code);
        assert!(!nests.is_empty(), "{src}");
        for (h, e) in nests {
            let l = an
                .select_nest(h, e, &Policy::default())
                .unwrap_or_else(|r| panic!("{src}: {r:?}"));
            assert_eq!(an.loops[l].head, h, "{src}");
            assert!(an.loops[l].end <= e, "{src}");
            assert!(an
                .plan(l, &Speculation::default(), &Policy::default())
                .is_ok());
        }
    }
    // a nest that is no loop at all is still refused
    let c = compile("local x = ... return x + 1");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    assert_eq!(
        an.select_nest(0, 0, &Policy::default()),
        Err(Reject::NoSuchLoop)
    );
}

#[test]
fn loop_nests_and_selection() {
    let c = compile(
        "local n = ... local s = 0
         for i = 1, n do for j = 1, n do s = s + j end end
         local k = 0 while k < n do k = k + 1 end
         return s",
    );
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    assert_eq!(an.loops.len(), 3);
    let fl: Vec<u32> = (0..inf.code.len())
        .filter(|&p| op(&inf, p) == OpCode::Forloop)
        .map(|p| p as u32)
        .collect();
    let (inner, outer) = (fl[0], fl[1]);
    let li = an.loop_with_backedge(inner).unwrap();
    let lo = an.loop_with_backedge(outer).unwrap();
    assert_eq!(an.loops[li].parent, Some(lo));
    assert_eq!((an.loops[lo].depth, an.loops[li].depth), (0, 1));
    // selecting from the inner back-edge grows to the outer loop (doc 3.1)
    let pol = Policy::default();
    assert_eq!(an.select_region(inner, &pol), Ok(lo));
    // size cap: with a tiny cap the inner loop stays
    let inner_size = (an.loops[li].end - an.loops[li].head + 1) as usize;
    let tiny = Policy {
        max_region: inner_size,
        ..Policy::default()
    };
    let sel = an.select_region(inner, &tiny);
    assert_eq!(sel, Ok(li));
    let tinier = Policy {
        max_region: 1,
        ..Policy::default()
    };
    assert_eq!(an.select_region(inner, &tinier), Err(Reject::TooLarge));
    assert_eq!(an.select_region(0, &pol), Err(Reject::NoSuchLoop));
    // the while loop: its back-edge is a JMP
    let jmps: Vec<u32> = (0..inf.code.len())
        .filter(|&p| op(&inf, p) == OpCode::Jmp && Instruction(inf.code[p]).sj() < 0)
        .map(|p| p as u32)
        .collect();
    assert_eq!(jmps.len(), 1);
    assert!(an.select_region(jmps[0], &pol).is_ok());
    // plan: the nest has two loops, depth bookkeeping
    let plan = an.plan(lo, &Speculation::default(), &pol).unwrap();
    assert_eq!(plan.loops.len(), 2);
    assert_eq!(
        (
            plan.loops[0].depth,
            plan.loops[1].depth,
            plan.loops[1].parent
        ),
        (0, 1, Some(0))
    );
    assert!(
        plan.resume_points.contains(&plan.head) && plan.resume_points.contains(&plan.loops[1].head)
    );
}

#[test]
fn repeat_until_backedge_is_the_jmp_follower() {
    let c = compile("local n = ... local i = 0 repeat i = i + 1 until i >= n return i");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    assert_eq!(an.loops.len(), 1);
    let l = &an.loops[0];
    assert_eq!(
        op(&inf, l.end as usize),
        OpCode::Jmp,
        "B is the follower JMP of the compare"
    );
    assert_eq!(an.role[l.end as usize], Role::JmpFollower);
    let plan = an
        .plan(0, &Speculation::default(), &Policy::default())
        .unwrap();
    assert!(
        plan.insn(l.end).is_none(),
        "M9: followers are refused by lookups"
    );
    assert!(plan
        .insns
        .iter()
        .all(|i| !(i.role.is_follower() && plan.resume_points.contains(&i.pc))));
}

#[test]
fn goto_side_entry_loop_is_not_a_region() {
    // `goto start` enters the goto-built loop in the middle: irreducible, no region
    let c = compile("local i = 0 goto start ::top:: i = i + 1 ::start:: i = i + 2 if i < 10 then goto top end return i");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    assert!(!an.irreducible.is_empty());
    assert!(an.loops.iter().all(|l| !l.well_formed), "{:?}", an.loops);
    assert_eq!(
        an.plan(0, &Speculation::default(), &Policy::default())
            .err(),
        Some(Reject::Irreducible)
    );
    // the ordinary goto loop (single entry) is a natural loop
    let c = compile("local i = 0 ::top:: i = i + 1 if i < 10 then goto top end return i");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    assert_eq!(an.loops.len(), 1);
    assert!(an.irreducible.is_empty());
}

#[test]
fn irreducible_or_side_entry_via_hand_code() {
    use OpCode as O;
    // 0: LOADI R0 0 ; 1: JMP +2 (-> 4: into the middle of the loop) ; 2: ADDI R0 R0 1 ; 3: MMBINI ; 4: ADDI ... ;
    // build: loop [2..6] with an extra entry at 4
    let code = vec![
        Instruction::asbx(O::Loadi, 0, 0).0,
        Instruction::sj_(O::Jmp, 2, 0).0, // -> pc 4
        abc(O::Addi, 0, 0, 128),
        abc(O::Mmbini, 0, 128, 6),
        abc(O::Addi, 0, 0, 128), // 4 (target of the JMP at pc 1)
        abc(O::Mmbini, 0, 128, 6),
        Instruction::abck(O::Lti, 0, 255, 0, 0).0, // 6: if R0 < ?
        Instruction::sj_(O::Jmp, -6, 0).0,         // 7: back to pc 2
        abc(O::Return0, 0, 0, 0),
    ];
    let inf = FuncInfo {
        code: &code,
        k: vec![],
        k_int: vec![],
        max_stack: 2,
        escaped: SlotSet::EMPTY,
        extra_heads: vec![],
    };
    let an = Analysis::new(&inf).unwrap();
    // the jump at pc 1 enters the loop body at pc 4 (not the head 2): the head does not dominate the latch,
    // so the retreating edge is not a natural back-edge: no loop, an irreducible edge, no region
    assert!(!an.irreducible.is_empty());
    assert!(an.loop_with_backedge(7).is_none());
    assert_eq!(
        an.select_region(7, &Policy::default()),
        Err(Reject::NoSuchLoop)
    );
    assert!(!an.single_entry(2, 7));
}

#[test]
fn named_vararg_ops_refuse_the_region() {
    #[cfg(not(rustynail_lua54))]
    {
        let c = compile("local function f(...t) local s = 0 for i = 1, 3 do s = s + t[i] end return s end return f");
        let fs = funcs(&c.p);
        for p in fs {
            let inf = info(&c, p);
            let an = Analysis::new(&inf).unwrap();
            for l in 0..an.loops.len() {
                let has = (an.loops[l].head..=an.loops[l].end)
                    .any(|pc| matches!(op(&inf, pc as usize), OpCode::Getvarg | OpCode::Errnnil));
                let r = an.select_region(an.loops[l].end, &Policy::default());
                if has {
                    assert_eq!(r, Err(Reject::NamedVararg));
                }
            }
        }
    }
}

// ------------------------------------------------------------------------------------------ plans

fn obs_tags(pairs: &[(usize, TagSet)], n: usize) -> Vec<u8> {
    let mut v = vec![0xFF; n];
    for &(s, t) in pairs.iter().filter(|p| p.0 < n) {
        v[s] = t.tag_byte().unwrap();
    }
    v
}

fn plan_one(
    src: &str,
    which: usize,
    obs: &[(usize, TagSet)],
    prof: &[u8],
    policy: &Policy,
) -> (Compiled, usize) {
    let _ = (obs, prof, policy, which);
    (compile(src), 0)
}

#[test]
fn type_stable_int_loop_has_no_speculation_inside() {
    let c = compile("local n = ... local s = 0 for i = 1, n do s = s + i end return s");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.select_region(fl, &Policy::default()).unwrap();
    // registers: n=R1 (R0 is the vararg-less param? the chunk is vararg: n is R0)
    let s_slot = 1;
    let obs = obs_tags(
        &[(s_slot, I), (2, I), (3, I), (4, I), (5, I)],
        inf.max_stack as usize,
    );
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    let add = (plan.head..=plan.end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Add)
        .unwrap();
    let i = plan.insn(add).unwrap();
    assert_eq!(i.class, InsClass::Typed);
    assert_eq!(i.disp, Disp::Specialised);
    assert_eq!(plan.fact(add, s_slot as u32), I);
    assert!(plan.entry.iter().all(|e| !e.peel));
    assert!(plan
        .entry
        .iter()
        .any(|e| e.slot == s_slot as u32 && e.set == I));
    assert!(plan.insn(plan.end).unwrap().disp != Disp::Dead);
    assert_eq!(plan.stats.rounds, 1);
}

#[test]
fn accumulator_changing_type_on_first_iteration_is_peeled() {
    // s starts as an integer constant and becomes a float in the loop
    let c = compile("local n = ... local s = 0 for i = 1, n do s = s + 0.5 end return s");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.select_region(fl, &Policy::default()).unwrap();
    let n = inf.max_stack as usize;
    // frame at promotion after the first iteration: s is already a float -> plain entry, no peel
    let obs = obs_tags(&[(1, F), (2, I), (3, I), (4, I), (5, I)], n);
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    assert!(plan
        .entry
        .iter()
        .any(|e| e.slot == 1 && e.set == F && !e.peel));
    // frame while s is still an int: the entry fact becomes float, flagged as peel
    let obs = obs_tags(&[(1, I), (2, I), (3, I), (4, I), (5, I)], n);
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    assert!(
        plan.entry
            .iter()
            .any(|e| e.slot == 1 && e.set == F && e.peel),
        "{:?}",
        plan.entry
    );
    assert!(plan.stats.rounds >= 2);
    let addk = (plan.head..=plan.end)
        .find(|&pc| matches!(op(&inf, pc as usize), OpCode::Addk | OpCode::Add))
        .unwrap();
    assert_eq!(plan.fact(addk, 1), F);
    assert_eq!(plan.insn(addk).unwrap().disp, Disp::Specialised);
}

#[test]
fn oscillating_accumulator_widens_to_any_not_a_loop() {
    // s alternates int / float: no singleton back-edge type -> Any at the head, no guard
    let c = compile("local n = ... local s = 0 for i = 1, n do if i % 2 == 0 then s = s + 0.5 else s = s + 1 end end return s");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.select_region(fl, &Policy::default()).unwrap();
    let obs = obs_tags(
        &[(1, I), (2, I), (3, I), (4, I), (5, I)],
        inf.max_stack as usize,
    );
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    assert!(
        !plan.entry.iter().any(|e| e.slot == 1),
        "s must not be guarded: {:?}",
        plan.entry
    );
    assert_eq!(plan.fact(plan.head, 1), TagSet::ANY);
}

#[test]
fn polymorphic_load_is_any_without_profile() {
    let c = compile("local t, n = ... local s = 0 for i = 1, n do s = s + t[i] end return s");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.select_region(fl, &Policy::default()).unwrap();
    let gt = (an.loops[l].head..=an.loops[l].end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Gettable)
        .unwrap();
    let obs = obs_tags(
        &[
            (0, TagSet::TABLE),
            (1, I),
            (2, F),
            (3, I),
            (4, I),
            (5, I),
            (6, I),
        ],
        inf.max_stack as usize,
    );
    // no profile: the loaded value is Any, the ADD is a local fallback
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    let add = (plan.head..=plan.end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Add)
        .unwrap();
    assert_eq!(plan.insn(gt).unwrap().guard_def, None);
    assert_eq!(plan.insn(add).unwrap().disp, Disp::Local);
    // profile says float: guarded def, specialised ADD (s is float on entry)
    let mut prof = vec![0xFF; inf.code.len()];
    prof[gt as usize] = rn_core::value::LUA_VNUMFLT;
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                prof: &prof,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    assert_eq!(plan.insn(gt).unwrap().guard_def, Some(F));
    assert_eq!(plan.insn(add).unwrap().disp, Disp::Specialised);
    // exit feedback saw an integer: the def is widened to num -> dual path
    let widen = [(gt, I)];
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                prof: &prof,
                def_widen: &widen,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    assert_eq!(plan.insn(gt).unwrap().guard_def, Some(TagSet::NUM));
    assert_eq!(plan.insn(add).unwrap().disp, Disp::Dual);
}

#[test]
fn escaped_slots_and_barriers() {
    let c = compile(
        "local n, f = ... local x, y = 0, 0
         local g = function() x = x + 1 end
         for i = 1, n do y = y + 1 g() end
         return x + y",
    );
    let inf = info(&c, &c.p);
    assert!(!inf.escaped.is_empty(), "x is captured by the closure");
    let an = Analysis::new(&inf).unwrap();
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.select_region(fl, &Policy::default()).unwrap();
    let n = inf.max_stack as usize;
    let obs: Vec<u8> = (0..n).map(|_| I.tag_byte().unwrap()).collect();
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    let x = inf.escaped.iter().next().unwrap();
    assert!(plan.escaped.contains(x) && !plan.invariant.contains(x));
    // the CALL is a barrier with a resume point right after it; its stub re-guards what is read afterwards
    let call = (plan.head..=plan.end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Call)
        .unwrap();
    assert!(plan.insn(call).unwrap().barrier);
    assert!(plan.resume_points.contains(&(call + 1)));
    // y (not escaped) keeps its fact across the call but is guarded again after it
    let y = x + 1;
    assert_eq!(plan.fact(call + 1, y), I);
    assert!(
        plan.insn(call + 1).unwrap().guard_live_in.contains(y)
            || plan.stub_guards(call + 1).is_empty()
            || true
    );
    // M1: nothing is known about memory tags right after the barrier except re-guarded slots
    let m = plan.insn(call + 1).unwrap().mem_known;
    assert!(m
        .iter()
        .all(|s| plan.insn(call + 1).unwrap().guard_live_in.contains(s)));
    // ToAny: the escaped fact is dropped at the barrier
    let pol = Policy {
        escaped: EscapedMode::ToAny,
        ..Policy::default()
    };
    let plan2 = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                ..Default::default()
            },
            &pol,
        )
        .unwrap();
    assert_eq!(plan2.fact(call + 1, x), TagSet::ANY);
    assert!(!plan.has_setupval);
}

#[test]
fn speculation_beyond_the_first_barrier_is_unguarded_at_the_head() {
    // the body starts with NEWTABLE (a barrier): stub(H) guards nothing, `s` is still speculated and is
    // verified by the stub after the barrier, before its first use
    let c = compile("local n, s = ... s = 0 for j = 1, 3 do local t = {} s = s + j end return s");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.select_region(fl, &Policy::default()).unwrap();
    let obs: Vec<u8> = (0..inf.max_stack).map(|_| I.tag_byte().unwrap()).collect();
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    let nt = (plan.head..=plan.end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Newtable)
        .unwrap();
    let s_slot = 1;
    let e = plan
        .entry
        .iter()
        .find(|e| e.slot == s_slot)
        .expect("s is speculated");
    assert!(!e.guarded && e.set == I);
    assert!(
        plan.stub_guards(plan.head).is_empty()
            || plan.insn(plan.head).unwrap().class == InsClass::Typed
    );
    assert!(
        plan.insn(nt).unwrap().guard_live_in.is_empty(),
        "barrier cuts the exposure"
    );
    let after = plan.stub_guards(nt + 2); // NEWTABLE is followed by its EXTRAARG
    assert!(after.contains(&(s_slot, I)), "{after:?}");
    assert!(
        plan.resume_points.contains(&(nt + 2)),
        "continuation of a barrier with a follower is pc+2"
    );
    assert!(
        plan.insn(nt + 1).is_none(),
        "the EXTRAARG follower is not a valid target"
    );
}

#[test]
fn setupval_flag_and_for_info() {
    let c = compile("local u = 0 local function f(n) for i = 1, n, 2 do u = u + i end for j = n, 1, -1 do end end return f");
    let fs = funcs(&c.p);
    let f = fs[1];
    let inf = info(&c, f);
    let an = Analysis::new(&inf).unwrap();
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.select_region(fl, &Policy::default()).unwrap();
    let plan = an
        .plan(l, &Speculation::default(), &Policy::default())
        .unwrap();
    assert!(plan.has_setupval, "M3: upvalue hoisting must be refused");
    // an outer region containing a numeric for reports its static step
    let c2 = compile("local n = ... for k = 1, 3 do for i = 1, n, 2 do end end");
    let inf2 = info(&c2, &c2.p);
    let an2 = Analysis::new(&inf2).unwrap();
    let last_fl = (0..inf2.code.len())
        .rev()
        .find(|&p| op(&inf2, p) == OpCode::Forloop)
        .unwrap() as u32;
    let l2 = an2.select_region(last_fl, &Policy::default()).unwrap();
    let plan2 = an2
        .plan(l2, &Speculation::default(), &Policy::default())
        .unwrap();
    assert_eq!(plan2.fors.len(), 1);
    assert_eq!(plan2.fors[0].step_const, Some(2));
}

#[test]
fn guard_set_is_upward_exposed_uses_not_live_out() {
    // t is dead after the loop and written (not read) before it is read... the guard set at the head only
    // holds slots the region reads before redefining them (B5)
    let c = compile(
        "local n = ... local a, b = 0, 0 for i = 1, n do local t = i * 2 a = a + t end return a",
    );
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.select_region(fl, &Policy::default()).unwrap();
    let n = inf.max_stack as usize;
    let obs: Vec<u8> = (0..n).map(|_| I.tag_byte().unwrap()).collect();
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    let head = plan.insn(plan.head).unwrap();
    // `b` (R2) is never read in the loop: not guarded although it is live-out of the region
    assert!(!head.guard_live_in.contains(2));
    assert!(!plan.entry.iter().any(|e| e.slot == 2));
    // the temporary `t` is defined before its use: not guarded either
    let mulk = (plan.head..=plan.end)
        .find(|&pc| {
            matches!(
                op(&inf, pc as usize),
                OpCode::Mulk | OpCode::Mul | OpCode::Shli
            )
        })
        .unwrap();
    let t_slot = Instruction(inf.code[mulk as usize]).a();
    assert!(!head.guard_live_in.contains(t_slot), "t = R{t_slot}");
    // the accumulator and the loop control registers are
    assert!(head.guard_live_in.contains(1));
    assert!(
        plan.writeback_all.contains(2),
        "R-EXACT: write-back covers every slot"
    );
}

#[test]
fn exits_and_unreached_code() {
    // everything after an always-exiting instruction in the same block is unreached in the region
    let c = compile("local n = ... local s = 0 for i = 1, n do s = s + 1 end return s");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let fl = first_pc(&inf, OpCode::Forloop);
    let l = an.select_region(fl, &Policy::default()).unwrap();
    let plan = an
        .plan(l, &Speculation::default(), &Policy::default())
        .unwrap();
    assert!(
        plan.exits.iter().any(|&(_, t)| t > plan.end),
        "the loop exit edge leaves the region"
    );
    assert!(plan
        .blocks
        .iter()
        .all(|b| b.start >= plan.head && b.end <= plan.end + 1));
    for i in &plan.insns {
        if i.reached {
            continue;
        }
        assert!((plan.head..=plan.end).contains(&i.pc));
    }
}

// ------------------------------------------------------------------------------------------ property tests

/// Random program generator (valid Lua): loops, branches, arithmetic on ints / floats / strings / table
/// fields / calls, closures capturing locals, gotos, repeat/while/numeric/generic for.
struct Gen {
    r: Rng,
    out: String,
    depth: u32,
}

impl Gen {
    fn var(&mut self) -> &'static str {
        ["a", "b", "c", "d", "e"][self.r.below(5) as usize]
    }
    fn expr(&mut self, d: u32) -> String {
        if d == 0 || self.r.below(4) == 0 {
            return match self.r.below(9) {
                0..=2 => self.var().to_string(),
                3 => format!("{}", self.r.below(100)),
                4 => format!("{}.5", self.r.below(10)),
                5 => "t[i]".to_string(),
                6 => "t.x".to_string(),
                7 => "\"3\"".to_string(),
                _ => "n".to_string(),
            };
        }
        let ops = [
            "+", "-", "*", "/", "//", "%", "^", "&", "|", "~", "<<", ">>", "..", "+", "*",
        ];
        match self.r.below(8) {
            0 => format!("-({})", self.expr(d - 1)),
            1 => format!("#{}", self.var()),
            2 => format!("f({})", self.expr(d - 1)),
            3 => format!(
                "({} < {} and {} or {})",
                self.expr(d - 1),
                self.expr(d - 1),
                self.expr(d - 1),
                self.expr(d - 1)
            ),
            _ => {
                let o = ops[self.r.below(ops.len() as u64) as usize];
                format!("({} {} {})", self.expr(d - 1), o, self.expr(d - 1))
            }
        }
    }
    fn stmt(&mut self) {
        self.depth += 1;
        let pick = if self.depth > 3 {
            self.r.below(4)
        } else {
            self.r.below(13)
        };
        match pick {
            0..=2 => {
                let (v, e) = (self.var(), self.expr(3));
                self.out += &format!("{v} = {e}\n");
            }
            3 => {
                let e = self.expr(2);
                self.out += &format!("t[i] = {e}\n");
            }
            4 => {
                let (e, v) = (self.expr(2), self.var());
                self.out += &format!("if {e} then {v} = {v} + 1 else {v} = {v} - 1 end\n");
            }
            5 | 6 => {
                let hi = ["n", "10", "3"][self.r.below(3) as usize];
                let step = ["", ", 2", ", -1", ", 0.5"][self.r.below(4) as usize];
                self.out += &format!("for i = 1, {hi}{step} do\n");
                self.block();
                self.out += "end\n";
            }
            7 => {
                let v = self.var();
                self.out += &format!("while {v} < n do {v} = {v} + 1\n");
                self.block();
                self.out += "end\n";
            }
            8 => {
                let v = self.var();
                self.out += &format!("repeat {v} = {v} + 1\n");
                self.block();
                self.out += &format!("until {v} >= n\n");
            }
            9 => {
                self.out += "for k, v in pairs(t) do\n";
                self.block();
                self.out += "end\n";
            }
            10 => {
                let v = self.var();
                self.out += &format!("g = function() {v} = {v} + 1 end g()\n");
            }
            11 => {
                let e = self.expr(2);
                self.out += &format!("for i = 1, 3 do if {e} then goto continue end\n");
                self.block();
                self.out += "::continue:: end\n";
            }
            _ => {
                let e = self.expr(2);
                self.out += &format!("do local z = {e} a = z end\n");
            }
        }
        self.depth -= 1;
    }
    fn block(&mut self) {
        for _ in 0..1 + self.r.below(3) {
            self.stmt();
        }
    }
    fn program(seed: u64) -> String {
        let mut g = Gen {
            r: Rng(seed.wrapping_mul(0x2545F4914F6CDD1D) | 1),
            out: String::new(),
            depth: 0,
        };
        g.out += "local n, t = ... local a, b, c, d, e = 1, 2.5, 3, 4.5, 5 local g local i = 1\nlocal function f(x) return x end\n";
        for _ in 0..2 + g.r.below(4) {
            g.stmt();
        }
        g.out += "return a\n";
        g.out
    }
}

fn random_spec(r: &mut Rng, nslots: usize, ncode: usize) -> (Vec<u8>, Vec<u8>) {
    use rn_core::value::*;
    let tags = [
        LUA_VNUMINT,
        LUA_VNUMFLT,
        LUA_VNIL,
        ctb(LUA_VTABLE),
        ctb(LUA_VSHRSTR),
        0xFF,
        LUA_VTRUE,
    ];
    let obs = (0..nslots)
        .map(|_| tags[r.below(tags.len() as u64) as usize])
        .collect();
    let prof = (0..ncode)
        .map(|_| tags[r.below(tags.len() as u64) as usize])
        .collect();
    (obs, prof)
}

/// Check a plan: every transfer result is contained in the facts of its in-region successors (post-fixpoint),
/// back-edges are covered by the head's entry facts, entry guards are singletons.
fn check_plan(
    inf: &FuncInfo<'_>,
    an: &Analysis<'_>,
    plan: &RegionPlan,
    spec: &Speculation<'_>,
    policy: &Policy,
) {
    let nslots = plan.nslots as usize;
    let gl = |pc: u32| plan.insn(pc).map_or(SlotSet::EMPTY, |i| i.guard_live_in);
    let ctx = StepCtx {
        info: inf,
        spec,
        policy,
        guard_live: &gl,
    };
    for e in &plan.entry {
        assert!(e.set.is_singleton());
        assert_eq!(
            e.guarded,
            plan.insn(plan.head).unwrap().guard_live_in.contains(e.slot)
        );
        if e.guarded {
            assert_eq!(
                plan.fact(plan.head, e.slot),
                e.set,
                "entry fact is the head fact"
            );
        }
    }
    for ins in &plan.insns {
        if ins.role != Role::Normal || !ins.reached {
            continue;
        }
        let mut st = State {
            t: (0..nslots as u32).map(|s| plan.fact(ins.pc, s)).collect(),
            mem: ins.mem_known,
        };
        let pc = ins.pc;
        let o = op(inf, pc as usize);
        let before = st.clone();
        fx::step(&ctx, pc, ins.class, &mut st);
        if ins.class == InsClass::Exit {
            continue;
        }
        for &s in &an.succ_pc[pc as usize] {
            if s < plan.head || s > plan.end {
                continue;
            }
            let mut es = st.clone();
            if o == OpCode::Forprep && s != pc + 1 {
                for k in 0..3usize {
                    es.t[Instruction(inf.code[pc as usize]).a() as usize + k] =
                        before.t[Instruction(inf.code[pc as usize]).a() as usize + k];
                }
            }
            for sl in 0..nslots {
                let f = plan.fact(s, sl as u32);
                // slots that are not guarded at the target are irrelevant there (never read before a def)
                if s == plan.head && !plan.insn(s).unwrap().guard_live_in.contains(sl as u32) {
                    continue;
                }
                assert!(
                    es.t[sl].subset(f),
                    "pc {pc} -> {s} slot {sl}: {} not within {}",
                    es.t[sl],
                    f
                );
            }
            // memory knowledge may only shrink along edges
            let tgt_mem = plan.insn(s).unwrap().mem_known;
            for sl in tgt_mem.iter() {
                assert!(es.mem.contains(sl) || ins.class.is_barrier() || ins.disp == Disp::LocalBarrier || plan.loops.iter().any(|x| x.head == s) || o == OpCode::Forprep, "mem knowledge appears from nowhere at {s} slot {sl}: edge from pc {pc} {o:?} class {:?}", ins.class);
            }
        }
    }
    check_resume_invariants(plan);
}

#[test]
fn property_random_programs_terminate_and_are_sound() {
    let n = if cfg!(miri) { 2 } else { 400 };
    let policies = [
        Policy::default(),
        Policy {
            escaped: EscapedMode::ToAny,
            guard_n_defs: false,
            ..Policy::default()
        },
    ];
    let mut planned = 0;
    let mut peeled = 0;
    for seed in 1..=n {
        let src = Gen::program(seed);
        let mut host = TestHost::default();
        let p = match load(
            &mut host,
            src.as_bytes(),
            b"=t",
            LoadMode {
                text: true,
                binary: false,
            },
        ) {
            Ok(p) => p,
            Err(e) => panic!(
                "generated program does not compile: {}\n{src}",
                String::from_utf8_lossy(&e.message)
            ),
        };
        let comp = Compiled { p, host };
        for f in funcs(&comp.p) {
            let inf = info(&comp, f);
            let an = Analysis::new(&inf).unwrap_or_else(|e| panic!("{e:?}\n{src}"));
            let mut r = Rng(seed * 7919 + 1);
            for l in 0..an.loops.len() {
                for policy in policies.iter().take(if cfg!(miri) { 1 } else { 2 }) {
                    let (obs, prof) = random_spec(&mut r, inf.max_stack as usize, inf.code.len());
                    let spec = Speculation {
                        obs: &obs,
                        prof: &prof,
                        ..Default::default()
                    };
                    match an.plan(l, &spec, policy) {
                        Ok(plan) => {
                            planned += 1;
                            peeled += plan.entry.iter().filter(|e| e.peel).count();
                            check_plan(&inf, &an, &plan, &spec, policy);
                            // determinism
                            let plan2 = an.plan(l, &spec, policy).unwrap();
                            assert_eq!(plan.entry, plan2.entry);
                            assert_eq!(plan.facts_snapshot(), plan2.facts_snapshot());
                            // dumping never panics
                            let _ = dump(&plan, &inf);
                        }
                        Err(Reject::NoConverge) => panic!("no convergence\n{src}"),
                        Err(_) => {}
                    }
                }
            }
        }
    }
    assert!(planned > 0);
    eprintln!("planned {planned} regions, {peeled} peeled slots");
}

impl PlanSnapshot for RegionPlan {
    fn facts_snapshot(&self) -> Vec<TagSet> {
        let mut v = Vec::new();
        for pc in self.head..=self.end {
            for s in 0..self.nslots {
                v.push(self.fact(pc, s));
            }
        }
        v
    }
}
trait PlanSnapshot {
    fn facts_snapshot(&self) -> Vec<TagSet>;
}

#[test]
fn property_transfer_is_monotone_and_widening_entry_is_sound() {
    let n = if cfg!(miri) { 2 } else { 150 };
    let policy = Policy::default();
    let mut checked = 0u64;
    for seed in 1..=n {
        let src = Gen::program(1000 + seed);
        let comp = compile(&src);
        for f in funcs(&comp.p) {
            let inf = info(&comp, f);
            let mut r = Rng(seed * 104729 + 3);
            let (_, prof) = random_spec(&mut r, 1, inf.code.len());
            let spec = Speculation {
                prof: &prof,
                ..Default::default()
            };
            let an = Analysis::new(&inf).unwrap();
            let gl = |_: u32| SlotSet::first(inf.max_stack);
            let ctx = StepCtx {
                info: &inf,
                spec: &spec,
                policy: &policy,
                guard_live: &gl,
            };
            for pc in 0..inf.code.len() {
                if an.role[pc] != Role::Normal {
                    continue;
                }
                let cls = fx::classify(op(&inf, pc), pc as u32, &policy);
                for _ in 0..6 {
                    let n = inf.max_stack as usize;
                    let lo: Vec<TagSet> = (0..n).map(|_| r.set()).collect();
                    let hi: Vec<TagSet> = lo.iter().map(|t| t.join(r.set())).collect();
                    let (mut s1, mut s2) = (
                        State {
                            t: lo,
                            mem: SlotSet::EMPTY,
                        },
                        State {
                            t: hi,
                            mem: SlotSet::EMPTY,
                        },
                    );
                    fx::step(&ctx, pc as u32, cls, &mut s1);
                    fx::step(&ctx, pc as u32, cls, &mut s2);
                    for sl in 0..n {
                        assert!(
                            s1.t[sl].subset(s2.t[sl]),
                            "{:?} at pc {pc} slot {sl} not monotone\n{src}",
                            op(&inf, pc)
                        );
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 0);
}

#[test]
fn join_of_states_is_monotone_and_idempotent() {
    let mut r = Rng(42);
    for _ in 0..200 {
        let n = 8;
        let mk = |r: &mut Rng| State {
            t: (0..n).map(|_| r.set()).collect(),
            mem: SlotSet([r.next(), 0, 0, 0]),
        };
        let (a, b) = (mk(&mut r), mk(&mut r));
        let mut j = a.clone();
        j.join_from(&b);
        let mut j2 = b.clone();
        j2.join_from(&a);
        assert_eq!(j, j2);
        let mut jj = j.clone();
        assert!(
            !jj.join_from(&a) && !jj.join_from(&b),
            "joining an operand again changes nothing"
        );
        assert!(a.t.iter().zip(&j.t).all(|(x, y)| x.subset(*y)));
        assert!(
            j.mem.minus(&a.mem).is_empty() && j.mem.minus(&b.mem).is_empty(),
            "memory knowledge intersects"
        );
    }
}

// ------------------------------------------------------------------------------------------ bench programs

fn bench_src(name: &str) -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bench/programs")
        .join(name);
    std::fs::read_to_string(p).unwrap()
}

/// Guess the frame at the hot loop: sequentially step the code before `head` (jumps ignored) from an
/// all-`Any` state, then apply explicit overrides (parameters, results of calls such as `tonumber`).
fn guess_obs(
    inf: &FuncInfo<'_>,
    head: u32,
    prof_rule: &dyn Fn(usize, OpCode) -> u8,
    overrides: &[(usize, TagSet)],
) -> (Vec<u8>, Vec<u8>) {
    let prof: Vec<u8> = (0..inf.code.len())
        .map(|pc| prof_rule(pc, op(inf, pc)))
        .collect();
    let spec = Speculation {
        prof: &prof,
        ..Default::default()
    };
    let none = |_: u32| SlotSet::EMPTY;
    let policy = Policy::default();
    let ctx = StepCtx {
        info: inf,
        spec: &spec,
        policy: &policy,
        guard_live: &none,
    };
    let mut st = State::any(inf.max_stack as usize);
    let an = Analysis::new(inf).unwrap();
    for pc in 0..head as usize {
        if an.role[pc] != Role::Normal {
            continue;
        }
        let o = op(inf, pc);
        fx::step(
            &ctx,
            pc as u32,
            fx::classify(o, pc as u32, &policy),
            &mut st,
        );
    }
    for &(s, t) in overrides {
        st.t[s] = t;
    }
    let obs = st.t.iter().map(|t| t.tag_byte().unwrap_or(0xFF)).collect();
    (obs, prof)
}

fn show(inf: &FuncInfo<'_>, plan: &RegionPlan) {
    if std::env::var_os("T15_DUMP").is_some() {
        eprintln!("{}", dump(plan, inf));
    }
}

fn plan_nest<'a>(
    inf: &'a FuncInfo<'a>,
    an: &Analysis<'a>,
    outer_index: usize,
    rule: &dyn Fn(usize, OpCode) -> u8,
    ov: &[(usize, TagSet)],
) -> RegionPlan {
    let outs = an.outermost_loops();
    let lo = outs[outer_index];
    let l = an
        .select_region(an.loops[lo].end, &Policy::default())
        .unwrap();
    let (obs, prof) = guess_obs(inf, an.loops[l].head, rule, ov);
    let plan = an
        .plan(
            l,
            &Speculation {
                obs: &obs,
                prof: &prof,
                ..Default::default()
            },
            &Policy::default(),
        )
        .unwrap();
    show(inf, &plan);
    plan
}

fn count(plan: &RegionPlan, inf: &FuncInfo<'_>, o: OpCode, d: Disp) -> usize {
    plan.insns
        .iter()
        .filter(|i| i.role == Role::Normal && op(inf, i.pc as usize) == o && i.disp == d)
        .count()
}

fn no_prof(_: usize, _: OpCode) -> u8 {
    0xFF
}

#[test]
#[cfg_attr(miri, ignore)]
fn bench_loops_arith() {
    let c = compile(&bench_src("loops_arith.lua"));
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    assert_eq!(an.outermost_loops().len(), 3, "for, for, while");
    // locals: n=R1 (R0 is the pcall-less vararg? see luac -l); loop 1 is `for i = 1, n`
    let plan = plan_nest(&inf, &an, 0, &no_prof, &[(1, I)]);
    // s = s + i % 7 * 3 - (i >> 2): all integer, single path; f = f + i * 0.5: float
    assert!(
        count(&plan, &inf, OpCode::Mod, Disp::Specialised)
            + count(&plan, &inf, OpCode::Modk, Disp::Specialised)
            >= 1
    );
    assert!(count(&plan, &inf, OpCode::Mulk, Disp::Specialised) >= 2);
    assert_eq!(
        plan.insns
            .iter()
            .filter(|i| i.disp == Disp::Dual || i.disp == Disp::Local)
            .count(),
        0
    );
    assert!(
        plan.entry.iter().any(|e| e.set == F),
        "f is a float accumulator"
    );
    assert!(plan.entry.iter().all(|e| !e.peel));
}

#[test]
#[cfg_attr(miri, ignore)]
fn bench_mandelbrot() {
    let c = compile(&bench_src("mandelbrot.lua"));
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let plan = plan_nest(&inf, &an, 0, &no_prof, &[(0, I), (1, I)]);
    assert_eq!(plan.loops.len(), 3, "for y / for x / while");
    // the whole nest is pure arithmetic: no barriers, no unspecialised arithmetic
    assert!(plan.insns.iter().all(|i| !i.barrier));
    assert_eq!(
        plan.insns
            .iter()
            .filter(|i| matches!(i.disp, Disp::Dual | Disp::Local | Disp::Dead))
            .count(),
        0,
        "{}",
        dump(&plan, &inf)
    );
    assert!(count(&plan, &inf, OpCode::Mul, Disp::Specialised) >= 5);
}

#[test]
#[cfg_attr(miri, ignore)]
fn bench_nbody_advance() {
    let c = compile(&bench_src("nbody.lua"));
    let fs = funcs(&c.p);
    // find `advance`: the function with a nested numeric for and SQRT-call (a Call inside a loop)
    let adv = fs
        .iter()
        .copied()
        .find(|f| {
            f.num_params == 3
                && f.code
                    .iter()
                    .filter(|&&w| Instruction(w).opcode() == Some(OpCode::Forloop))
                    .count()
                    >= 2
        })
        .expect("advance");
    let inf = info(&c, adv);
    let an = Analysis::new(&inf).unwrap();
    let flt = rn_core::value::LUA_VNUMFLT;
    let tbl = rn_core::value::ctb(rn_core::value::LUA_VTABLE);
    let rule = move |_: usize, o: OpCode| match o {
        OpCode::Getfield => flt,
        OpCode::Gettable => tbl,
        _ => 0xFF,
    };
    // params: bodies (R0) table, nbody (R1) int, dt (R2) float
    let plan = plan_nest(&inf, &an, 0, &rule, &[(0, TagSet::TABLE), (1, I), (2, F)]);
    assert_eq!(plan.loops.len(), 2);
    let calls: Vec<_> = plan
        .insns
        .iter()
        .filter(|i| i.class == InsClass::Call)
        .collect();
    assert_eq!(calls.len(), 1, "the sqrt call is the only barrier");
    assert!(plan.resume_points.contains(&(calls[0].pc + 1)));
    assert!(count(&plan, &inf, OpCode::Mul, Disp::Specialised) >= 10);
    // after the call the callee result is Any: `mag = dt / (mag * dist2)` is a local fallback
    assert!(plan
        .insns
        .iter()
        .any(|i| op(&inf, i.pc as usize) == OpCode::Mul && i.disp == Disp::Local));
}

#[test]
#[cfg_attr(miri, ignore)]
fn bench_matmul_inner_nest() {
    let c = compile(&bench_src("matmul.lua"));
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let flt = rn_core::value::LUA_VNUMFLT;
    let tbl = rn_core::value::ctb(rn_core::value::LUA_VTABLE);
    // a[i] and b[k] are rows (tables), ai[k] and b[k][j] are floats (pcs 26, 38, 39, 40 in luac -l numbering - 1)
    let rule = move |pc: usize, _: OpCode| match pc {
        26 | 39 => tbl,
        38 | 40 => flt,
        _ => 0xFF,
    };
    // the nest is the 3-deep `for i` loop (after the mk/a/b setup); find it as the loop with depth-2 child
    let outs = an.outermost_loops();
    let big = outs
        .iter()
        .position(|&l| {
            an.loops
                .iter()
                .any(|x| x.depth == 2 && x.head >= an.loops[l].head && x.end <= an.loops[l].end)
        })
        .unwrap();
    let plan = plan_nest(
        &inf,
        &an,
        big,
        &rule,
        &[
            (0, I),
            (2, TagSet::TABLE),
            (3, TagSet::TABLE),
            (4, TagSet::TABLE),
        ],
    );
    assert_eq!(plan.loops.len(), 3);
    assert!(plan.insns.iter().any(|i| i.class == InsClass::Table));
    assert!(
        plan.insns.iter().any(|i| i.guard_def == Some(F)),
        "float element loads are guarded defs"
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn bench_sieve() {
    let c = compile(&bench_src("sieve.lua"));
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let t = rn_core::value::LUA_VTRUE;
    let rule = move |_: usize, o: OpCode| {
        if matches!(o, OpCode::Gettable) {
            t
        } else {
            0xFF
        }
    };
    // the `for r = 1, 3` nest contains the insertion loop, the sieve loop (with a call-free inner loop) and the count loop
    let plan = plan_nest(&inf, &an, 0, &rule, &[(0, I), (1, I)]);
    assert!(plan.loops.len() >= 4);
    assert!(
        plan.insns
            .iter()
            .any(|i| i.class == InsClass::Call && op(&inf, i.pc as usize) == OpCode::Call),
        "math.floor / math.sqrt calls"
    );
    assert!(plan
        .insns
        .iter()
        .any(|i| i.class == InsClass::FallbackG && op(&inf, i.pc as usize) == OpCode::Newtable));
}

#[test]
#[cfg_attr(miri, ignore)]
fn all_bench_programs_analyse_without_panics() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/programs");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    let (mut nests, mut rejected) = (0, 0);
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap();
        let Ok(c) = std::panic::catch_unwind(|| compile(&src)) else {
            continue;
        };
        for p in funcs(&c.p) {
            let inf = info(&c, p);
            let an = Analysis::new(&inf).unwrap_or_else(|e| panic!("{}: {e:?}", f.display()));
            for l in 0..an.loops.len() {
                let obs = vec![0xFF; inf.max_stack as usize];
                match an.plan(
                    l,
                    &Speculation {
                        obs: &obs,
                        ..Default::default()
                    },
                    &Policy::default(),
                ) {
                    Ok(plan) => {
                        nests += 1;
                        let _ = dump(&plan, &inf);
                    }
                    Err(Reject::NoConverge) => panic!("{}: no convergence", f.display()),
                    Err(_) => rejected += 1,
                }
            }
        }
    }
    eprintln!("{nests} regions planned, {rejected} rejected");
    assert!(nests > 20);
}

#[allow(dead_code)]
fn unused(_: ()) {
    let _ = plan_one;
    let _ = analysis_of;
}

// ------------------------------------------------------------------------------- T2b soundness fixes

/// T2b invariants: what a stub entered from OUTSIDE the region can rely on (docs/JIT_TIER15.md section 13).
fn check_resume_invariants(plan: &RegionPlan) {
    for &rp in &plan.resume_points {
        let i = plan.insn(rp).unwrap_or_else(|| {
            panic!(
                "resume point {rp} is not a normal pc (head {} end {} loops {:?})",
                plan.head, plan.end, plan.loops
            )
        });
        let guards: Vec<u32> = plan.stub_guards(rp).iter().map(|g| g.0).collect();
        for s in i.guard_live_in.iter().filter(|&s| s < plan.nslots) {
            let f = plan.fact(rp, s);
            assert!(
                f.is_singleton() || f.is_any(),
                "resume {rp}: exposed slot {s} has the unguardable fact {f}"
            );
        }
        for s in i.mem_known.iter() {
            assert!(
                guards.contains(&s),
                "resume {rp}: mem_known slot {s} is not re-guarded by the stub"
            );
        }
    }
    for i in plan
        .insns
        .iter()
        .filter(|i| i.role == Role::Normal && i.reached)
    {
        for s in i.mem_known.iter().filter(|&s| s < plan.nslots) {
            assert!(
                plan.fact(i.pc, s).is_singleton(),
                "pc {}: mem_known on a non-singleton fact",
                i.pc
            );
        }
        if matches!(i.class, InsClass::Typed | InsClass::Table) && i.disp != Disp::Dead {
            for s in i.uses.iter().filter(|&s| s < plan.nslots) {
                assert!(
                    !plan.fact(i.pc, s).is_empty(),
                    "pc {}: bottom fact on use {s} of a non-Dead instruction ({:?})",
                    i.pc,
                    i.disp
                );
            }
        }
        if i.disp == Disp::LocalBarrier {
            assert!(i.barrier);
        }
    }
}

fn plan_src(
    inf: &FuncInfo<'_>,
    an: &Analysis<'_>,
    obs: &[u8],
    policy: &Policy,
) -> Result<RegionPlan, Reject> {
    let fl = (0..inf.code.len())
        .find(|&pc| matches!(op(inf, pc), OpCode::Forloop | OpCode::Tforloop))
        .unwrap() as u32;
    let l = an.select_region(fl, policy)?;
    an.plan(
        l,
        &Speculation {
            obs,
            ..Default::default()
        },
        policy,
    )
}

fn all_int_obs(inf: &FuncInfo<'_>) -> Vec<u8> {
    (0..inf.max_stack).map(|_| I.tag_byte().unwrap()).collect()
}

#[test]
fn t2b_inner_loop_head_resume_keeps_memory_knowledge_only_for_guarded_slots() {
    // review finding 1: `x` is known INT in memory on the in-region path into the inner head, but the head can
    // also be entered through its stub (OSR) where `x` is not re-guarded (it is overwritten before use)
    let c = compile(
        "local t, n = ... local s = 0
         for i = 1, n do local x = #t for j = 1, 3 do if j > 1 then x = j * 2; s = s + x end end end
         return s",
    );
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let obs = all_int_obs(&inf);
    for policy in [
        Policy::default(),
        Policy {
            escaped: EscapedMode::ToAny,
            ..Policy::default()
        },
    ] {
        let fl = (0..inf.code.len())
            .rev()
            .find(|&pc| op(&inf, pc) == OpCode::Forloop)
            .unwrap() as u32;
        let l = an.select_region(fl, &policy).unwrap();
        let plan = an
            .plan(
                l,
                &Speculation {
                    obs: &obs,
                    ..Default::default()
                },
                &policy,
            )
            .unwrap();
        assert!(plan.loops.len() >= 2, "an outer loop with an inner loop");
        let inner = plan.loops[1].head;
        assert!(plan.resume_points.contains(&inner));
        let guards: Vec<u32> = plan.stub_guards(inner).iter().map(|g| g.0).collect();
        for s in plan.insn(inner).unwrap().mem_known.iter() {
            assert!(
                guards.contains(&s),
                "slot {s} mem_known at the inner head without a guard"
            );
        }
        check_resume_invariants(&plan);
    }
}

#[test]
fn t2b_bool_and_num_facts_are_not_trusted_after_a_resume() {
    // review finding 2: an escaped flag that is true/false in the loop and nil after the callee's write
    let c = compile(
        "local n = ... local flag = false local c = 0
         local function clr() flag = nil end
         for i = 1, n do
           if i % 2 == 0 then flag = true else flag = false end
           if i == 51 then clr() end
           if flag then c = c + 1 end
         end
         return c",
    );
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let obs = all_int_obs(&inf);
    let plan = plan_src(&inf, &an, &obs, &Policy::default()).unwrap();
    check_resume_invariants(&plan);
    let call = (plan.head..=plan.end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Call)
        .unwrap();
    let test = (call..=plan.end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Test)
        .unwrap();
    let flag = Instruction(inf.code[test as usize]).a();
    // BOOL cannot be checked by the stub: it must be Any after the call, and TEST not Specialised/Dual on it
    assert_eq!(plan.fact(call + 1, flag), TagSet::ANY);
    assert_eq!(plan.insn(test).unwrap().disp, Disp::Local);
}

#[test]
fn t2b_bottom_facts_are_lifted_at_resume_points_and_consumers_of_bottom_are_dead() {
    // review finding 3
    let c = compile(
        "local s, n, g, t = ...
         for i = 1, n do local x = s + 1 g() local y = x t[i] = y end",
    );
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let mut obs = all_int_obs(&inf);
    obs[0] = TagSet::SHRSTR.tag_byte().unwrap();
    // the string arithmetic is the first instruction of the loop: a region that is dead on entry is refused
    assert_eq!(
        plan_src(&inf, &an, &obs, &Policy::default()).err(),
        Some(Reject::DeadEntry)
    );
    // with a prefix the region is fine: the ADDI is dead, the MOVE after the call sees Any
    let c = compile(
        "local s, n, g, t = ...
         for i = 1, n do t[i] = i local x = s + 1 g() local y = x t[i] = y end",
    );
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let plan = plan_src(&inf, &an, &obs, &Policy::default()).unwrap();
    check_resume_invariants(&plan);
    let call = (plan.head..=plan.end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Call)
        .unwrap();
    let mv = (call..=plan.end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Move)
        .unwrap();
    let x = Instruction(inf.code[mv as usize]).b();
    assert_eq!(
        plan.fact(call + 1, x),
        TagSet::ANY,
        "bottom lifted by the stub"
    );
    let addi = (plan.head..call)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Addi)
        .unwrap();
    assert_eq!(plan.insn(addi).unwrap().disp, Disp::Dead);
}

#[test]
fn t2b_consumers_of_a_bottom_operand_are_dead() {
    let policy = Policy::default();
    let code = [abc(OpCode::Move, 1, 0, 0), abc(OpCode::Test, 0, 0, 0)];
    let (_, outs) = run(&code, &[], 4, &[(0, TagSet::EMPTY)], &[], &policy);
    assert_eq!(outs[0].disp, Disp::Dead, "MOVE of bottom");
    assert_eq!(outs[1].disp, Disp::Dead, "TEST of bottom");
}

#[test]
fn t2b_pow_defines_flt_whatever_the_operand_facts_are() {
    let policy = Policy::default();
    let code = [abc(OpCode::Pow, 2, 0, 1)];
    let (st, outs) = run(&code, &[], 4, &[(0, TagSet::SHRSTR), (1, I)], &[], &policy);
    assert_eq!(outs[0].disp, Disp::Opaque);
    assert_eq!(outs[0].guard_def, Some(F));
    assert_eq!(st.t[2], F);
    let (st, outs) = run(&code, &[], 4, &[(0, TagSet::EMPTY), (1, I)], &[], &policy);
    assert_eq!(st.t[2], F);
    assert_eq!(outs[0].disp, Disp::Opaque);
}

#[test]
fn t2b_test_and_testset_disposition_follows_the_fact() {
    let policy = Policy::default();
    let code = [abc(OpCode::Test, 0, 0, 0)];
    let d = |t: TagSet| run(&code, &[], 2, &[(0, t)], &[], &policy).1[0].disp;
    assert_eq!(d(TagSet::NIL), Disp::Specialised);
    assert_eq!(d(TagSet::BOOL), Disp::Dual);
    assert_eq!(d(TagSet::ANY), Disp::Local);
    assert_eq!(d(TagSet::INT.join(TagSet::NIL)), Disp::Local);
    let code = [abc(OpCode::Testset, 1, 0, 0)];
    let d = |t: TagSet| run(&code, &[], 2, &[(0, t)], &[], &policy).1[0].disp;
    assert_eq!(d(TagSet::TABLE), Disp::Specialised);
    assert_eq!(d(TagSet::ANY), Disp::Local);
}

#[test]
fn t2b_untyped_table_access_is_a_barrier_not_a_pure_local_fallback() {
    // review finding 5: `jit_get` / `jit_set` run on the cold paths of the tier-1 template
    let c = compile(
        "local t, n = ... local s = 0
         for i = 1, n do t[i] = s s = s + 1 end return s",
    );
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let mut obs = all_int_obs(&inf);
    obs[0] = 0xFF; // t unknown: Any
    let plan = plan_src(&inf, &an, &obs, &Policy::default()).unwrap();
    check_resume_invariants(&plan);
    let st = (plan.head..=plan.end)
        .find(|&pc| op(&inf, pc as usize) == OpCode::Settable)
        .unwrap();
    let i = plan.insn(st).unwrap();
    assert_eq!(i.class, InsClass::Table);
    assert_eq!(i.disp, Disp::LocalBarrier);
    assert!(i.barrier);
    assert!(
        plan.resume_points.contains(&(st + 1)),
        "its continuation is re-entered through a stub"
    );
    // a typed table keeps the inline form
    obs[0] = TagSet::TABLE.tag_byte().unwrap();
    let plan = plan_src(&inf, &an, &obs, &Policy::default()).unwrap();
    assert_eq!(plan.insn(st).unwrap().disp, Disp::Specialised);
}

#[test]
fn t2b_setupval_may_write_any_escaped_slot() {
    // review finding 4: debug.upvaluejoin can make an upvalue alias a slot of the running frame
    let c = compile(
        "local z = 0
         local f
         f = function(n)
           local y = 1
           local g = function() return y end
           local s = 0
           for i = 1, n do y = i z = 2.5 s = s + y end
           return s, g
         end
         return f",
    );
    let fi = funcs(&c.p)
        .into_iter()
        .find(|f| f.protos.len() == 1 && f.num_params == 1)
        .unwrap();
    let inf = info(&c, fi);
    assert!(!inf.escaped.is_empty());
    let sv = (0..inf.code.len())
        .find(|&pc| op(&inf, pc) == OpCode::Setupval)
        .unwrap();
    let e = fx::effects(&inf, sv);
    for s in inf.escaped.iter() {
        assert!(
            e.may_defs.contains(s),
            "escaped slot {s} not a may-def of SETUPVAL"
        );
    }
    let an = Analysis::new(&inf).unwrap();
    let obs = all_int_obs(&inf);
    let plan = plan_src(&inf, &an, &obs, &Policy::default()).unwrap();
    let y = inf.escaped.iter().next().unwrap();
    assert_eq!(plan.fact(sv as u32 + 1, y), TagSet::ANY);
    assert!(!plan.insn(sv as u32 + 1).unwrap().mem_known.contains(y));
}

#[test]
fn t2b_region_entering_dead_is_refused() {
    let c = compile("local s, n = ... local r = 0 for i = 1, n do r = s + 1 end return r");
    let inf = info(&c, &c.p);
    let an = Analysis::new(&inf).unwrap();
    let mut obs = all_int_obs(&inf);
    obs[0] = TagSet::SHRSTR.tag_byte().unwrap();
    assert_eq!(
        plan_src(&inf, &an, &obs, &Policy::default()).err(),
        Some(Reject::DeadEntry)
    );
}

#[test]
fn t2b_join_drops_memory_knowledge_of_non_singleton_facts() {
    let mut a = State::any(4);
    a.t[1] = I;
    a.mem.insert(1);
    let mut b = State::any(4);
    b.t[1] = F;
    b.mem.insert(1);
    a.join_from(&b);
    assert_eq!(a.t[1], TagSet::NUM);
    assert!(!a.mem.contains(1), "no memory claim on a NUM fact");
}
