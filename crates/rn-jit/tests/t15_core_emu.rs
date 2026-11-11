//! Regions of integer loops on synthetic frames, run by the emulator (Miri-runnable) and natively, against the
//! tier-1 code of the same function: same exit, same frame (every slot, temporaries included), same budget, same
//! `savedpc` and `trap` (docs/JIT_TIER15.md invariants T-EXACT and T-COUNT), for random initial values, budgets
//! (block-count exits), pending interrupts (poll exits), non-integer parameters (entry guard failures), and every
//! allocation mode.
#![cfg(feature = "jit")]

use rn_compiler::testhost::TestHost;
use rn_compiler::{load, LoadMode};
use rn_core::jit_layout::callinfo as ci;
use rn_core::protodata::{Const, ProtoData};
use rn_core::value::{Value, LUA_VNIL, LUA_VNUMFLT, LUA_VNUMINT};
use rn_jit::emu::{self, Env, Stop};
use rn_jit::lower::{lower, LowerCfg, Lowered};
use rn_jit::t15::runtime::{lower_with_regions, scan_nests, FuncMeta, NestPlan, OptFlags};
use rn_jit::t15::SlotSet;

const FAKE_CODE: usize = 0x10_0000;
const NSLOT: usize = 40;
const PRE: usize = 2;

#[derive(Clone)]
struct Mem {
    frame: Vec<u64>,
    ci: Vec<u64>,
    thread: Vec<u64>,
    k: Vec<u64>,
    cnt: i32,
    irq: Vec<u8>,
}

impl Mem {
    fn new() -> Mem {
        Mem {
            frame: vec![0; (NSLOT + PRE) * 2],
            ci: vec![0; ci::SIZE.div_ceil(8)],
            thread: vec![0; 512],
            k: vec![0; 128],
            cnt: i32::MAX,
            irq: vec![0],
        }
    }
    /// The constants array of the running closure (region code reads float constants from memory).
    fn set_k(&mut self, consts: &[Value]) {
        assert!(consts.len() * 2 <= self.k.len());
        for (i, v) in consts.iter().enumerate() {
            self.k[2 * i] = u64::from(v.tag());
            self.k[2 * i + 1] = match (v.as_int(), v.as_float()) {
                (Some(x), _) => x as u64,
                (_, Some(x)) => x.to_bits(),
                _ => v.as_string().map_or(0, |s| s.as_ptr() as u64),
            };
        }
    }
    fn set_reg(&mut self, r: usize, tag: u8, payload: u64) {
        self.frame[(PRE + r) * 2] = u64::from(tag);
        self.frame[(PRE + r) * 2 + 1] = payload;
    }
    fn put<T: Copy>(buf: &mut [u64], off: usize, v: T) {
        assert!(off + std::mem::size_of::<T>() <= buf.len() * 8);
        // SAFETY: bounds checked above; unaligned write inside the buffer.
        unsafe {
            buf.as_mut_ptr()
                .cast::<u8>()
                .add(off)
                .cast::<T>()
                .write_unaligned(v);
        }
    }
    fn get<T: Copy>(buf: &[u64], off: usize) -> T {
        assert!(off + std::mem::size_of::<T>() <= buf.len() * 8);
        // SAFETY: bounds checked above.
        unsafe {
            buf.as_ptr()
                .cast::<u8>()
                .add(off)
                .cast::<T>()
                .read_unaligned()
        }
    }
    fn setup_ci(&mut self, callstatus: u32, trap: i32) {
        Self::put(&mut self.ci, ci::CALLSTATUS, callstatus);
        Self::put(&mut self.ci, ci::TRAP, trap);
        Self::put(&mut self.ci, ci::PREVIOUS, 0x7000usize);
        Self::put(&mut self.ci, ci::FUNC, 5usize);
        Self::put(&mut self.ci, ci::KCACHE, self.k.as_ptr() as usize);
    }
    fn env(&mut self) -> Env {
        Env {
            frame: self.frame.as_mut_ptr().cast::<u8>().wrapping_add(PRE * 16),
            ci: self.ci.as_mut_ptr().cast(),
            thread: self.thread.as_mut_ptr().cast(),
            k: self.k.as_ptr().cast(),
            cnt: &raw mut self.cnt,
        }
    }
}

struct Func {
    code: Vec<u32>,
    consts: Vec<Value>,
    max_stack: u32,
}

/// The first nested function of `src` (`local function f(...) ... end return f`).
fn func_of(src: &str) -> Func {
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
    let f: &ProtoData<u32> = &p.protos[0];
    let consts =
        f.k.iter()
            .map(|c| match c {
                Const::Int(i) => Value::int(*i),
                Const::Float(x) => Value::float(*x),
                _ => panic!("emulator tests use numeric constants only"),
            })
            .collect();
    Func {
        code: f.code.clone(),
        consts,
        max_stack: u32::from(f.max_stack_size),
    }
}

fn cfg(f: &Func, irq: *const u8) -> LowerCfg<'_> {
    LowerCfg {
        calls: None,
        code_addr: FAKE_CODE,
        interrupt_addr: irq as usize,
        consts: &f.consts,
        t15_warmup: None,
    }
}

fn tier1(f: &Func, irq: *const u8) -> Lowered {
    lower(&f.code, cfg(f, irq)).expect("lowers")
}

/// Frame tags at promotion for the entry speculation: 0 all integers, 1 all floats, 2 odd slots floats, 3 slots >= 6
/// floats (so that the float templates are exercised whatever the program's own slot layout is).
fn obs_for(variant: u32, nslots: usize, nplans: usize) -> Vec<Vec<u8>> {
    let tag = |s: usize| match variant {
        0 => LUA_VNUMINT,
        1 => LUA_VNUMFLT,
        2 if s % 2 == 1 => LUA_VNUMFLT,
        3 if s >= 6 => LUA_VNUMFLT,
        _ => LUA_VNUMINT,
    };
    vec![(0..nslots).map(tag).collect(); nplans]
}

fn region(f: &Func, irq: *const u8, opt: &str) -> Lowered {
    region_obs(f, irq, opt, 0)
}

fn region_obs(f: &Func, irq: *const u8, opt: &str, obsv: u32) -> Lowered {
    let nests = scan_nests(&f.code);
    assert!(!nests.is_empty(), "no loop");
    let plans: Vec<NestPlan> = nests
        .iter()
        .enumerate()
        .map(|(i, &(head, end))| NestPlan {
            nest: i as u32,
            head,
            end,
        })
        .collect();
    // the frame at promotion: everything an integer
    let obs = obs_for(obsv, f.max_stack as usize, plans.len());
    let meta = FuncMeta {
        max_stack: f.max_stack,
        escaped: SlotSet::EMPTY,
        obs,
        prof: vec![0xFF; f.code.len()],
        intr: Vec::new(),
        opt: OptFlags::parse(&format!("{opt},+force")),
    };
    let low = lower_with_regions(&f.code, cfg(f, irq), &plans, Some(&meta)).expect("lowers");
    if std::env::var_os("T4B_DUMP").is_some() {
        for (i, o) in low.prog.ops.iter().enumerate() {
            eprintln!("{i:5} {o:?}");
        }
    }
    low
}

fn run_emu(low: &Lowered, m: &mut Mem) -> Stop {
    let (_, l) = *low.resume.iter().find(|(pc, _)| *pc == 0).expect("entry");
    let env = m.env();
    // SAFETY: the buffers in `m` are large enough for every offset the templates use.
    unsafe { emu::run(&low.prog, l, &env, 5_000_000) }
}

#[cfg(not(miri))]
fn run_native(low: &Lowered, m: &mut Mem) -> u32 {
    use rn_jit::mem::CodeArena;
    use rn_jit::x86::{emit, thunk_code, Thunk};
    let em = emit(&low.prog).expect("assembles");
    let (_, l) = *low.resume.iter().find(|(pc, _)| *pc == 0).unwrap();
    let off = em.label_off[l as usize].expect("bound");
    let mut arena = CodeArena::new();
    let tc = thunk_code();
    let mut tb = arena.alloc(tc.len(), 16).unwrap();
    arena.writer(&mut tb).unwrap().write_at(0, &tc);
    let mut cb = arena.alloc(em.code.len(), 16).unwrap();
    arena.writer(&mut cb).unwrap().write_at(0, &em.code);
    arena.commit().unwrap();
    // SAFETY: `tb` holds the thunk (committed); `Thunk` is its signature.
    let thunk: Thunk = unsafe { arena.entry(&tb) }.unwrap();
    let env = m.env();
    // SAFETY: as in the emulator; the target is a resume label of the committed block.
    let code = unsafe {
        thunk(
            env.thread,
            env.ci,
            env.frame,
            env.cnt,
            (cb.addr() + off) as *const u8,
        )
    };
    drop(arena);
    code
}

const PROGS: &[(&str, &str)] = &[
    (
        "divk",
        "local function f(n, a, b, c)
           for i = 1, n do a = a % 7 + b // -3 + i % 5 + (c // 2); b = b - a; c = c + i * 3 end
           return a
         end return f",
    ),
    (
        "straight",
        "local function f(n, a, b, c)
           for i = 1, n do a = a + i * 3 - b; b = b ~ a; c = (a & 255) + (b >> 3) end
           return a
         end return f",
    ),
    (
        "nested_branch",
        "local function f(n, a, b, c)
           for i = 1, n do
             for j = 1, i do
               if i < j + 2 then a = a + 1 elseif i == j then b = b - i else c = c + a end
               if a >= 7 then a = a - 5 end
             end
           end
           return a
         end return f",
    ),
    (
        "while_neg",
        "local function f(n, a, b, c)
           local k = 0
           while k < n do k = k + 1; a = -a + k; if a > 1000 then break end; b = ~b + a end
           return a
         end return f",
    ),
    (
        "repeat_mod",
        "local function f(n, a, b, c)
           local i = 0
           repeat i = i + 1; a = a + i % 5; c = c * 3 until i >= n or a > 100000
           return a
         end return f",
    ),
    (
        "float_mix",
        "local function f(n, a, b, c)
           local x = 0.5
           for i = 1, n do a = a + i; x = x * 1.5 + a end
           return a
         end return f",
    ),
    (
        "fl_arith",
        "local function f(n, a, b, c)
           local x, y = a + 0.5, b * 0.25
           for i = 1, n do x = x * 1.5 + i; y = y - x / 3; y = -y; x = x - 1 + y * 2; y = x - y; x = i / x end
           return y
         end return f",
    ),
    (
        "fl_cmp",
        "local function f(n, a, b, c)
           local x, y, z = a + 0.25, c * 1.0, 1
           for i = 1, n do
             x = x + 0.75
             if x > 3 then x = x - 3.5 elseif x < -2.0 then x = -x end
             if x == 1.25 then y = y + 1.0 end
             if x <= y then z = z + 1 end
             if y >= 2.5 then y = 0.5 end
             if x ~= x then y = y + 100 end
             if y < b then z = z - 1 end
           end
           return z + y
         end return f",
    ),
    (
        "fl_for",
        "local function f(n, a, b, c)
           local s = 0.0
           for x = a, n, c do s = s + x end
           return s
         end return f",
    ),
    (
        "fl_for_f",
        "local function f(n, a, b, c)
           local s = 0
           for x = a * 1.0, n + 0.5, c * 1.0 do s = s + x end
           return s
         end return f",
    ),
    (
        "fl_special",
        "local function f(n, a, b, c)
           local nan, inf, nz = 0.0 / 0.0, 1e308 * 10, -0.0
           local x, y = a * nz, b * 1.0
           for i = 1, n do
             x = x + nz
             y = y * inf
             if y ~= y then y = c * 1.0 end
             if x == nan then x = 2.0 end
             x = x / (i - 3) + y / inf
             y = nan * 0 + y
           end
           return x
         end return f",
    ),
    (
        "fl_cross_class",
        "local function f(n, a, b, c)
           local x, y, z = 0, a + 0.5, 0.25
           for i = 1, n do
             x = i * 3 + x % 7
             x = x ~ i
             x = y * 1.5
             z = z + x
             z = z * x - 0.5
             x = i + 1
             y = y + z * 0.125
           end
           return z + x
         end return f",
    ),
    (
        "fl_for_xreg_slot",
        "local function f(n, a, b, c)
           local s, t = 0, 0.0
           for r = 1, 3 do
             for i = 1, n do s = s + i end
             local x = a + 0.5
             for q = 1, n do
               for p = 1, n do x = x * 1.0001 + 0.5; x = x - 0.25 * x + 1.5 end
             end
             t = t + x
           end
           return s + t
         end return f",
    ),
    (
        "fl_xmm_exhaust",
        "local function f(n, a, b, c)
           local v1, v2, v3, v4, v5, v6 = a + 0.5, 1.5, 2.5, 3.5, 4.5, 5.5
           local v7, v8, v9, v10, v11, v12 = 6.5, 7.5, 8.5, 9.5, 10.5, 11.5
           local v13, v14, v15, v16, v17, v18 = 12.5, 13.5, 14.5, 15.5, 16.5, 17.5
           for i = 1, n do
             v1 = v1 * 0.999 + v2; v2 = v2 + v3 * 0.5; v3 = v3 - v4 * 0.25; v4 = v4 + v5 * 0.125
             v5 = v5 * 0.99 + v6; v6 = v6 + v7 * 0.0625; v7 = v7 - v8 * 0.03125; v8 = v8 + v9 * 0.015625
             v9 = v9 * 0.98 + v10; v10 = v10 + v11 * 0.5; v11 = v11 - v12 * 0.25; v12 = v12 + v13 * 0.125
             v13 = v13 * 0.97 + v14; v14 = v14 + v15 * 0.5; v15 = v15 - v16 * 0.25; v16 = v16 + v17 * 0.125
             v17 = v17 * 0.96 + v18; v18 = v18 - v1 * 0.0001
           end
           return v1 + v18
         end return f",
    ),
    (
        "consts",
        "local function f(n, a, b, c)
           for i = 1, n do a = a + 1000000007; b = b * 2654435761 & 0xffffffff; c = c - 7 + (a >> 60) end
           return a
         end return f",
    ),
];

fn rnd_state(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

fn compare(name: &str, opt: &str, obsv: u32) {
    let src = PROGS.iter().find(|(n, _)| *n == name).unwrap().1;
    let f = func_of(src);
    let mut seed = 0x9e37_79b9_7f4a_7c15u64 ^ ((name.len() as u64) << 32);
    let interesting = [0i64, 1, -1, 2, 7, 100, -100, i64::MAX, i64::MIN, 1 << 40];
    let cases = if cfg!(miri) { 6 } else { 150 };
    for case in 0..cases {
        let pick = |s: &mut u64| -> i64 {
            match rnd_state(s) % 3 {
                0 => interesting[(rnd_state(s) % interesting.len() as u64) as usize],
                1 => (rnd_state(s) % 21) as i64 - 10,
                _ => rnd_state(s) as i64,
            }
        };
        let mut m = Mem::new();
        m.set_k(&f.consts);
        let nres = (rnd_state(&mut seed) % 3) as u32;
        m.setup_ci(nres, i32::from(rnd_state(&mut seed).is_multiple_of(9)));
        m.set_reg(0, LUA_VNUMINT, rnd_state(&mut seed) % 12);
        for r in 1..4 {
            m.set_reg(r, LUA_VNUMINT, pick(&mut seed) as u64);
        }
        // sometimes a non-integer parameter: the entry guards fail and tier 1 runs on
        match rnd_state(&mut seed) % 14 {
            0 => m.set_reg(2, LUA_VNIL, 0),
            1 => m.set_reg(1, LUA_VNUMFLT, 1.5f64.to_bits()),
            _ => {}
        }
        if name.starts_with("fl_") {
            // floats with edge values; the `for` programs get step / start values that terminate quickly
            let specials = [
                0.0f64,
                -0.0,
                0.5,
                -2.5,
                1.5,
                3.0,
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                9007199254740992.0,
                1e300,
                -1e-300,
            ];
            let small = [-3i64, -1, 0, 1, 2, 5];
            let steps = [1.0f64, 2.0, -1.0, -2.0, 3.0, 0.25, -0.5, 1.5];
            for r in 1..4usize {
                let sel = rnd_state(&mut seed);
                if name.starts_with("fl_for") {
                    if r == 3 {
                        let v = steps[(sel % steps.len() as u64) as usize];
                        if sel.is_multiple_of(2) && v.fract() == 0.0 {
                            m.set_reg(r, LUA_VNUMINT, v as i64 as u64);
                        } else {
                            m.set_reg(r, LUA_VNUMFLT, v.to_bits());
                        }
                    } else if sel.is_multiple_of(3) {
                        m.set_reg(
                            r,
                            LUA_VNUMFLT,
                            specials[2 + (sel / 3 % 3) as usize].to_bits(),
                        );
                    } else {
                        m.set_reg(r, LUA_VNUMINT, small[(sel / 3 % 6) as usize] as u64);
                    }
                } else {
                    match sel % 4 {
                        0 => m.set_reg(r, LUA_VNUMINT, small[(sel / 4 % 6) as usize] as u64),
                        1 | 2 => m.set_reg(
                            r,
                            LUA_VNUMFLT,
                            specials[(sel / 4 % specials.len() as u64) as usize].to_bits(),
                        ),
                        _ => {}
                    }
                }
            }
        }
        m.cnt = if rnd_state(&mut seed).is_multiple_of(3) {
            (rnd_state(&mut seed) % 40) as i32
        } else {
            i32::MAX
        };
        m.irq[0] = u8::from(rnd_state(&mut seed).is_multiple_of(20));
        let mut m1 = m.clone();
        let mut m2 = m.clone();
        let low1 = tier1(&f, m1.irq.as_ptr());
        let low2 = region_obs(&f, m2.irq.as_ptr(), opt, obsv);
        let s1 = run_emu(&low1, &mut m1);
        let s2 = run_emu(&low2, &mut m2);
        assert_eq!(s1, s2, "{name} `{opt}` case {case}: exit");
        if m1.frame != m2.frame {
            let d: Vec<String> = (0..m1.frame.len())
                .filter(|&k| m1.frame[k] != m2.frame[k])
                .map(|k| format!("word {k}: {:#x} vs {:#x}", m1.frame[k], m2.frame[k]))
                .collect();
            panic!("{name} `{opt}` case {case}: frame: {}", d.join("; "));
        }
        assert_eq!(m1.cnt, m2.cnt, "{name} `{opt}` case {case}: budget");
        for off in [ci::SAVEDPC, ci::TRAP] {
            assert_eq!(
                Mem::get::<u64>(&m1.ci, off),
                Mem::get::<u64>(&m2.ci, off),
                "{name} `{opt}` case {case}: ci field {off}"
            );
        }
        assert_eq!(m1.thread, m2.thread, "{name} `{opt}` case {case}: thread");
        #[cfg(not(miri))]
        {
            let mut m3 = m.clone();
            let low3 = region_obs(&f, m3.irq.as_ptr(), opt, obsv);
            let code = run_native(&low3, &mut m3);
            let Stop::Ret(want) = s2 else {
                panic!("emulator did not finish")
            };
            assert_eq!(code, want, "{name} `{opt}` case {case}: native exit");
            assert_eq!(
                m2.frame, m3.frame,
                "{name} `{opt}` case {case}: native frame"
            );
            assert_eq!(m2.cnt, m3.cnt, "{name} `{opt}` case {case}: native budget");
            assert_eq!(
                m2.thread, m3.thread,
                "{name} `{opt}` case {case}: native thread"
            );
        }
    }
}

#[test]
fn regions_match_tier1_emulated_and_native() {
    for (name, _) in PROGS {
        for opt in [
            "",
            "-ra",
            "+randra=3",
            "+randra=11",
            "+flushall",
            "+exitstorm=1",
            "+exitstorm=2,+randra=5",
            "+exitstorm=7",
        ] {
            let variants: &[u32] = if name.starts_with("fl_") {
                &[0, 1, 2, 3]
            } else {
                &[0]
            };
            for &v in variants {
                compare(name, opt, v);
            }
        }
    }
}

/// The exit storm (docs/JIT_TIER15.md 22.2) on a few loops: small enough for Miri (the full option matrix of
/// `regions_match_tier1_emulated_and_native` is not).
#[test]
fn exit_storm_regions_match_tier1_emulated() {
    for (name, _) in PROGS.iter().take(4) {
        for opt in ["+exitstorm=1", "+exitstorm=2,+randra=5", "+exitstorm=7"] {
            compare(name, opt, 0);
        }
    }
}

#[test]
fn region_code_is_generated_without_helper_calls() {
    use rn_jit::ir::Op;
    let f = func_of(PROGS[0].1);
    let m = Mem::new();
    let t1 = tier1(&f, m.irq.as_ptr());
    let r = region(&f, m.irq.as_ptr(), "");
    if rn_core::jit_layout::LUA54
        || std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"))
    {
        assert_eq!(r.sig, t1.sig);
        return;
    }
    assert!(r.prog.ops.len() > t1.prog.ops.len(), "no region code");
    assert_eq!(r.sig, t1.sig, "block structure must not change");
    // (the only helper region code may call is the failure report of the debug-assertion budget check)
    let verify_fail = rn_jit::t15::stubs::jit_t15_verify_fail as *const () as usize as u64;
    assert!(
        !r.prog
            .ops
            .iter()
            .any(|o| matches!(o, Op::CallH { f, .. } if *f != verify_fail)
                || matches!(o, Op::JmpR(_)))
    );
}

// ------------------------------------------------------------------------------------------------ T12: helpers
//
// Regions of loops that contain fallbacks with helper calls (`calls: Some`): `CALL`, `LEN`, `GETTABLE`, `SETTABLE`,
// `POW`, `CONCAT`, `NEWTABLE`. Native helpers are replaced by a seeded model (`Model`) that, per call, randomly
//
// * finishes (`H_DONE`, writing results of the type the tier-1 template assumes), asks the interpreter to do the
//   instruction (`H_EXIT`) or reports an error (`H_ERR`);
// * for helpers that can run arbitrary Lua code (`CALL`, allocating helpers): overwrites a random frame slot with a
//   value of a random type (what `debug.setlocal` from a callee does) and/or *moves the Lua stack*.
//
// The same seed gives the same decisions in the tier-1 run and in the region run, as long as both make the same helper
// calls in the same order. Both runs are traced: every time execution reaches a region pc (the `RegionPoint`s: the
// tier-1 label, or the region label with its register-resident slots read from the machine registers) the frame, the
// budget word, the `CallInfo` and the thread are recorded; the traces must be identical, and so must the final state.

use rn_core::jit_helpers as jh;
use rn_core::value::LUA_VTRUE;
use rn_jit::emu::{Emu, HelperOut, Helpers, Until};
use rn_jit::ir::V;
use rn_jit::lower::calls::CallEnv;
use std::collections::HashMap;

const FAKE_CALL: u64 = 0xCA11_0001;
const FAKE_TAIL: u64 = 0xCA11_0002;

/// A process-wide zeroed block standing for the caller's `CallInfo`.
fn dummy_ci() -> usize {
    // (kept as a `Box` in the static, not leaked and stored as an integer: Miri reports a block that only an integer in a static knows about as a
    // memory leak, and the boxed slice does not move)
    static D: std::sync::OnceLock<Box<[u64]>> = std::sync::OnceLock::new();
    D.get_or_init(|| vec![0u64; ci::SIZE.div_ceil(8) + 8].into_boxed_slice()).as_ptr() as usize
}

fn calls_cfg(f: &Func, irq: *const u8) -> LowerCfg<'_> {
    LowerCfg {
        calls: Some(CallEnv {
            stack_off: 0,
            proto_code: 0,
            proto_k: 0,
            call_helper: FAKE_CALL,
            ccall_helper: 0,
            armm: false,
            tail_helper: FAKE_TAIL,
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
        }),
        ..cfg(f, irq)
    }
}

fn tier1_calls(f: &Func, irq: *const u8) -> Lowered {
    lower(&f.code, calls_cfg(f, irq)).expect("lowers")
}

fn region_calls(f: &Func, irq: *const u8, opt: &str, obsv: u32) -> Lowered {
    let nests = scan_nests(&f.code);
    assert!(!nests.is_empty(), "no loop");
    let plans: Vec<NestPlan> = nests
        .iter()
        .enumerate()
        .map(|(i, &(head, end))| NestPlan {
            nest: i as u32,
            head,
            end,
        })
        .collect();
    let obs = obs_for(obsv, f.max_stack as usize, plans.len());
    let meta = FuncMeta {
        max_stack: f.max_stack,
        escaped: SlotSet::EMPTY,
        obs,
        prof: vec![0xFF; f.code.len()],
        intr: Vec::new(),
        opt: OptFlags::parse(&format!("{opt},+force")),
    };
    lower_with_regions(&f.code, calls_cfg(f, irq), &plans, Some(&meta)).expect("lowers")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Call,
    Tail,
    Get,
    Set,
    Len,
    Pow,
    /// NEWTABLE, SETLIST, CONCAT, CLOSURE
    Alloc,
    Other,
}

fn kind_of(f: u64) -> Kind {
    let a = |p: *const ()| p as usize as u64;
    if f == FAKE_CALL {
        Kind::Call
    } else if f == FAKE_TAIL {
        Kind::Tail
    } else if f == a(jh::jit_get as *const ()) {
        Kind::Get
    } else if f == a(jh::jit_set as *const ()) {
        Kind::Set
    } else if f == a(jh::jit_len as *const ()) {
        Kind::Len
    } else if f == a(jh::jit_pow as *const ()) {
        Kind::Pow
    } else if f == a(jh::jit_newtable as *const ())
        || f == a(jh::jit_setlist as *const ())
        || f == a(jh::jit_concat as *const ())
        || f == a(jh::jit_closure as *const ())
    {
        Kind::Alloc
    } else {
        Kind::Other
    }
}

/// The seeded helper model (see the section comment).
struct Model {
    seed: u64,
    calls: u64,
    /// Buffers of moved stacks (boxed vectors: stable addresses).
    moved: Vec<Vec<u64>>,
    /// Log of decisions (printed on a mismatch).
    log: Vec<String>,
    /// Allow slot mutations and stack moves (else only done/exit/error decisions).
    chaos: bool,
    /// Calls of a registered `math` function with numeric operands that the model answers with the real result,
    /// without consuming random decisions (T6): the slow path of an intrinsic call and the libm helper land here.
    math_calls: u64,
}

impl Model {
    fn new(seed: u64, chaos: bool) -> Model {
        Model {
            seed,
            calls: 0,
            moved: Vec::new(),
            log: Vec::new(),
            chaos,
            math_calls: 0,
        }
    }
    /// A call of a registered `math` function (through the tier-1 `CALL` helper or `jit_t15_math`) with the operands
    /// the inline forms take: the real result of `lmathlib.c`, `thread.top` as `pre_call_native_fast` leaves it.
    ///
    /// # Safety
    /// `frame`, `env` as in [`Helpers::call`].
    unsafe fn try_math(&mut self, f: u64, a: u32, env: &Env, frame: *mut u8) -> Option<HelperOut> {
        use rn_core::jit_hooks::{self, thread as thr};
        let rd = |s: usize| -> (u8, u64) {
            // SAFETY: `s` is a register of the synthetic frame.
            unsafe {
                (
                    *frame.add(s * 16),
                    frame.add(s * 16 + 8).cast::<u64>().read_unaligned(),
                )
            }
        };
        let helper = rn_jit::t15::codegen::jit_t15_math as *const () as usize as u64;
        let (slot, kind, nargs) = if f == FAKE_CALL {
            let ins = rn_core::instruction::Instruction(a);
            let s = ins.a() as usize;
            let (tag, pay) = rd(s);
            if tag != rn_core::value::LUA_VLCF || ins.c() != 2 {
                return None;
            }
            let k = jit_hooks::intrinsic_of(pay as usize)?;
            (s, k, ins.b() as usize - 1)
        } else if f == helper {
            ((a & 0xFF) as usize, ((a >> 8) & 0xFF) as u8, 1)
        } else {
            return None;
        };
        let want = rn_jit::t15::intrinsics::Intr::from_index(kind)?.nargs() as usize;
        if nargs != want {
            return None;
        }
        let mut args = [(0u8, 0u64); 2];
        for (k, arg) in args.iter_mut().enumerate().take(nargs) {
            *arg = rd(slot + 1 + k);
            if arg.0 != LUA_VNUMINT && arg.0 != LUA_VNUMFLT {
                return None;
            }
        }
        let num = |v: (u8, u64)| -> f64 {
            if v.0 == LUA_VNUMINT {
                v.1 as i64 as f64
            } else {
                f64::from_bits(v.1)
            }
        };
        let pushnumint = |d: f64| -> (u8, u64) {
            if (-9223372036854775808.0..9223372036854775808.0).contains(&d) {
                (LUA_VNUMINT, d as i64 as u64)
            } else {
                (LUA_VNUMFLT, d.to_bits())
            }
        };
        let flt = |d: f64| (LUA_VNUMFLT, d.to_bits());
        let a0 = args[0];
        let res = match kind {
            jit_hooks::intr::SQRT => flt(num(a0).sqrt()),
            jit_hooks::intr::SIN => flt(num(a0).sin()),
            jit_hooks::intr::COS => flt(num(a0).cos()),
            jit_hooks::intr::EXP => flt(num(a0).exp()),
            jit_hooks::intr::LOG => flt(num(a0).ln()),
            jit_hooks::intr::ABS if a0.0 == LUA_VNUMINT => {
                (LUA_VNUMINT, (a0.1 as i64).wrapping_abs() as u64)
            }
            jit_hooks::intr::ABS => flt(num(a0).abs()),
            jit_hooks::intr::FLOOR if a0.0 == LUA_VNUMINT => a0,
            jit_hooks::intr::FLOOR => pushnumint(num(a0).floor()),
            jit_hooks::intr::CEIL if a0.0 == LUA_VNUMINT => a0,
            jit_hooks::intr::CEIL => pushnumint(num(a0).ceil()),
            jit_hooks::intr::MIN | jit_hooks::intr::MAX => {
                let a1 = args[1];
                // lua_compare(L, i, imin, LUA_OPLT): exact for same-type operands (the only ones tested)
                let lt = |x: (u8, u64), y: (u8, u64)| -> bool {
                    if x.0 == LUA_VNUMINT && y.0 == LUA_VNUMINT {
                        (x.1 as i64) < (y.1 as i64)
                    } else {
                        num(x) < num(y)
                    }
                };
                if kind == jit_hooks::intr::MIN {
                    if lt(a1, a0) {
                        a1
                    } else {
                        a0
                    }
                } else if lt(a0, a1) {
                    a1
                } else {
                    a0
                }
            }
            _ => return None,
        };
        // SAFETY: `slot` is a register of the synthetic frame; `env.ci` / `env.thread` are the live buffers.
        unsafe {
            put_slot(frame, slot, res);
            let func = env.ci.add(ci::FUNC).cast::<usize>().read_unaligned();
            env.thread
                .add(thr::TOP)
                .cast::<usize>()
                .write_unaligned(func + slot + 2);
        }
        self.math_calls += 1;
        Some(HelperOut {
            result: jh::H_DONE,
            new_frame: None,
        })
    }
    fn rnd(&mut self) -> u64 {
        let mut x = self.seed ^ self.calls.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.seed = self.seed.wrapping_add(0xA076_1D64_78BD_642F);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    }
    /// A random value `(tag, payload)`; `only` restricts the kind (0 any, 1 int, 2 float).
    fn value(&mut self, only: u8) -> (u8, u64) {
        let r = self.rnd();
        let kind = if only != 0 {
            only
        } else {
            [1, 1, 1, 2, 3, 4][(r % 6) as usize]
        };
        match kind {
            1 => (
                LUA_VNUMINT,
                [0, 1, 2, 7, 100, (r >> 8) % 50, u64::MAX, 1 << 40][((r >> 20) % 8) as usize],
            ),
            2 => (
                LUA_VNUMFLT,
                [0.5f64, -2.0, 3.0, 1e300][((r >> 20) % 4) as usize].to_bits(),
            ),
            3 => (LUA_VNIL, 0),
            _ => (LUA_VTRUE, 0),
        }
    }
}

/// Writes a value into slot `s` of the frame at `frame`.
///
/// # Safety
/// `frame` is a frame base with at least `s + 1` slots.
unsafe fn put_slot(frame: *mut u8, s: usize, v: (u8, u64)) {
    // SAFETY: per the contract.
    unsafe {
        frame.add(s * 16).write(v.0);
        frame.add(s * 16 + 8).cast::<u64>().write_unaligned(v.1);
    }
}

impl Helpers for Model {
    unsafe fn call(&mut self, f: u64, a: u32, _b: u32, _env: &Env, frame: *mut u8) -> HelperOut {
        // SAFETY: the contract of `Helpers::call`.
        if let Some(out) = unsafe { self.try_math(f, a, _env, frame) } {
            return out;
        }
        self.calls += 1;
        let verify_fail = rn_jit::t15::stubs::jit_t15_verify_fail as *const () as usize as u64;
        assert!(
            f != verify_fail,
            "region code reported an internal error (+verify type check or budget invariant) at pc {a}"
        );
        let kind = kind_of(f);
        if std::env::var_os("T12_TRACE").is_some() {
            eprintln!("    helper {kind:?} a={a:#x}");
        }
        let ins = rn_core::instruction::Instruction(a);
        let r = self.rnd();
        // status
        let (can_exit, can_move) = match kind {
            Kind::Call | Kind::Tail => (false, true),
            Kind::Alloc => (true, true),
            _ => (true, false),
        };
        let status = match r % 100 {
            0..=5 if can_exit => jh::H_EXIT,
            6..=9 => jh::H_ERR,
            _ => jh::H_DONE,
        };
        let mut new_frame = None;
        let mut cur = frame;
        if status == jh::H_DONE {
            let a_reg = ins.a() as usize;
            match kind {
                Kind::Len => {
                    let v = self.value(1);
                    // SAFETY: `a_reg` is a register of the synthetic frame.
                    unsafe { put_slot(cur, a_reg, v) };
                }
                Kind::Pow => {
                    let v = self.value(2);
                    // SAFETY: as above.
                    unsafe { put_slot(cur, a_reg, v) };
                }
                Kind::Get | Kind::Alloc => {
                    let v = self.value(0);
                    // SAFETY: as above.
                    unsafe { put_slot(cur, a_reg, v) };
                }
                Kind::Call => {
                    for k in 0..ins.c().saturating_sub(1) as usize {
                        let v = self.value(0);
                        // SAFETY: as above.
                        unsafe { put_slot(cur, a_reg + k, v) };
                    }
                }
                _ => {}
            }
            if self.chaos && can_move {
                let r2 = self.rnd();
                if r2.is_multiple_of(3) {
                    // what a callee's `debug.setlocal` does: any slot of the frame gets a value of any type
                    let slot = (r2 >> 8) as usize % (NSLOT / 2);
                    let v = self.value(0);
                    // SAFETY: `slot < NSLOT`.
                    unsafe { put_slot(cur, slot, v) };
                    self.log
                        .push(format!("call {}: slot {slot} := {v:?}", self.calls));
                }
                if (r2 >> 40).is_multiple_of(4) {
                    // the Lua stack moves
                    // SAFETY: the frame buffer has `PRE` slots below the base and `NSLOT` above.
                    let mut nb: Vec<u64> = unsafe {
                        std::slice::from_raw_parts(
                            cur.sub(PRE * 16).cast::<u64>(),
                            (NSLOT + PRE) * 2,
                        )
                        .to_vec()
                    };
                    let nf = nb.as_mut_ptr().cast::<u8>().wrapping_add(PRE * 16);
                    self.moved.push(nb);
                    new_frame = Some(nf);
                    cur = nf;
                    self.log.push(format!("call {}: stack moved", self.calls));
                }
            }
        }
        let _ = cur;
        self.log
            .push(format!("call {} {kind:?} -> {status}", self.calls));
        HelperOut {
            result: status,
            new_frame,
        }
    }
}

/// Label -> (pc, region?, register-resident slots with their registers).
type Stops = HashMap<u32, (u32, bool, Vec<(u32, V)>, Vec<(u32, u8)>)>;

/// One observation of a run at a region pc.
#[derive(Clone, PartialEq, Debug)]
struct Event {
    pc: u32,
    /// Seen at region code (registers overlaid) or at tier-1 code.
    region: bool,
    frame: Vec<u64>,
    budget: i32,
    ci: Vec<u64>,
    thread: Vec<u64>,
    /// The synthetic table pool (T5), empty otherwise.
    pool: Vec<u64>,
}

struct Trace {
    events: Vec<Event>,
    end: Until,
    frame: Vec<u64>,
    cnt: i32,
    ci: Vec<u64>,
    thread: Vec<u64>,
    calls: u64,
    log: Vec<String>,
    pool: Vec<u64>,
}

fn read_frame(base: *mut u8) -> Vec<u64> {
    // SAFETY: every frame buffer has `PRE` slots below the base and `NSLOT` above (`Mem::new`, `Model`).
    unsafe {
        std::slice::from_raw_parts(base.sub(PRE * 16).cast::<u64>(), (NSLOT + PRE) * 2).to_vec()
    }
}

/// Runs `low` from its entry and records an [`Event`] at every `(label, pc, region?, registers)` of `stops`.
fn trace(
    low: &Lowered,
    stops: &Stops,
    m: &mut Mem,
    model: &mut Model,
    pool: Option<&Pool>,
) -> Trace {
    let (_, l) = *low.resume.iter().find(|(pc, _)| *pc == 0).expect("entry");
    let env = m.env();
    let labels: Vec<u32> = stops.keys().copied().collect();
    let mut emu = Emu::new(&low.prog, l);
    let mut events = Vec::new();
    let end = loop {
        // SAFETY: the buffers in `m` and `model.moved` are live and large enough for every offset the code uses.
        match unsafe { emu.run(&low.prog, &labels, &env, 400_000, model) } {
            Until::Label(lab) => {
                let (pc, region, rr, rx) = &stops[&lab];
                if std::env::var_os("T12_TRACE").is_some() {
                    eprintln!("    at pc {pc} region={region} rr={rr:?} rx={rx:?}");
                }
                let fp = emu.frame(&env);
                let mut frame = read_frame(fp);
                if *region {
                    for &(slot, v) in rr {
                        let i = (PRE + slot as usize) * 2;
                        frame[i] = (frame[i] & !0xff) | u64::from(LUA_VNUMINT);
                        frame[i + 1] = emu.r[v as usize];
                    }
                    for &(slot, x) in rx {
                        let i = (PRE + slot as usize) * 2;
                        frame[i] = (frame[i] & !0xff) | u64::from(LUA_VNUMFLT);
                        frame[i + 1] = emu.x[x as usize];
                    }
                }
                events.push(Event {
                    pc: *pc,
                    region: *region,
                    frame,
                    budget: if *region {
                        emu.r[5] as u32 as i32
                    } else {
                        m.cnt
                    },
                    ci: m.ci.clone(),
                    thread: m.thread.clone(),
                    pool: pool.map(Pool::snapshot).unwrap_or_default(),
                });
            }
            s @ Until::Stop(_) => break s,
        }
    };
    let fp = emu.frame(&env);
    Trace {
        events,
        end,
        frame: read_frame(fp),
        cnt: m.cnt,
        ci: m.ci.clone(),
        thread: m.thread.clone(),
        calls: model.calls,
        log: std::mem::take(&mut model.log),
        pool: pool.map(Pool::snapshot).unwrap_or_default(),
    }
}

/// Label -> (pc, region?, registers) for the tier-1 run (`pcs` = region pcs that have a tier-1 label).
fn t1_stops(low: &Lowered, range: (u32, u32)) -> Stops {
    (range.0..=range.1)
        .filter_map(|pc| {
            low.pc_labels[pc as usize].map(|l| (l, (pc, false, Vec::new(), Vec::new())))
        })
        .collect()
}

/// ... and for the region run: both the region labels and the (raw) tier-1 labels of the same pcs.
fn region_stops(low: &Lowered, plain: &Lowered, range: (u32, u32)) -> Stops {
    let mut m = HashMap::new();
    for p in &low.points {
        if p.pc < range.0 || p.pc > range.1 || plain.pc_labels[p.pc as usize].is_none() {
            continue;
        }
        m.insert(p.t1, (p.pc, false, Vec::new(), Vec::new()));
        if let Some(r) = p.reg {
            m.insert(r, (p.pc, true, p.rr.clone(), p.rx.clone()));
        }
    }
    m
}

fn diff_events(name: &str, opt: &str, case: u32, t: &Trace, r: &Trace) {
    let ctx = |what: &str| {
        format!(
            "{name} `{opt}` case {case}: {what}\nhelper log (tier 1):\n  {}\nhelper log (region):\n  {}",
            t.log.join("\n  "),
            r.log.join("\n  ")
        )
    };
    for (i, (a, b)) in t.events.iter().zip(&r.events).enumerate() {
        if a.pc != b.pc {
            panic!(
                "{}",
                ctx(&format!(
                    "event {i}: tier 1 at pc {}, region run at pc {} (region label: {})",
                    a.pc, b.pc, b.region
                ))
            );
        }
        if a.frame != b.frame {
            let d: Vec<String> = (0..a.frame.len())
                .filter(|&k| a.frame[k] != b.frame[k])
                .map(|k| {
                    format!(
                        "word {k} (slot {}): {:#x} vs {:#x}",
                        (k / 2).wrapping_sub(PRE),
                        a.frame[k],
                        b.frame[k]
                    )
                })
                .collect();
            panic!(
                "{}",
                ctx(&format!(
                    "event {i} at pc {} (region label: {}): frame differs: {}",
                    a.pc,
                    b.region,
                    d.join("; ")
                ))
            );
        }
        if a.budget != b.budget {
            panic!(
                "{}",
                ctx(&format!(
                    "event {i} at pc {} (region label: {}): budget {} vs {}",
                    a.pc, b.region, a.budget, b.budget
                ))
            );
        }
        if a.pool != b.pool {
            let d: Vec<String> = (0..a.pool.len().min(b.pool.len()))
                .filter(|&k| a.pool[k] != b.pool[k])
                .map(|k| format!("pool word {k}: {:#x} vs {:#x}", a.pool[k], b.pool[k]))
                .collect();
            panic!(
                "{}",
                ctx(&format!(
                    "event {i} at pc {} (region label: {}): table memory differs: {}",
                    a.pc,
                    b.region,
                    d.join("; ")
                ))
            );
        }
        if a.ci != b.ci || a.thread != b.thread {
            panic!(
                "{}",
                ctx(&format!(
                    "event {i} at pc {} (region label: {}): ci/thread differ",
                    a.pc, b.region
                ))
            );
        }
    }
    assert_eq!(
        t.events.len(),
        r.events.len(),
        "{}",
        ctx("number of events")
    );
    assert_eq!(t.end, r.end, "{}", ctx("end"));
    assert_eq!(t.frame, r.frame, "{}", ctx("final frame"));
    assert_eq!(t.cnt, r.cnt, "{}", ctx("final budget"));
    assert_eq!(t.ci, r.ci, "{}", ctx("final ci"));
    assert_eq!(t.thread, r.thread, "{}", ctx("final thread"));
    assert_eq!(t.pool, r.pool, "{}", ctx("final table memory"));
    assert_eq!(t.calls, r.calls, "{}", ctx("number of helper calls"));
}

const CALL_PROGS: &[(&str, &str)] = &[
    (
        "fl_mix",
        "local function f(n, a, b, c, t, g)
           local x = 0.5
           for i = 1, n do a = a + i; x = x * 1.5 + a end
           return x
         end return f",
    ),
    (
        "fl_arith",
        "local function f(n, a, b, c, t, g)
           local x, y = a + 0.5, c * 0.25
           for i = 1, n do x = x * 1.5 + i; y = y - x / 3; y = -y; x = x - 1 + y * 2; y = x - y; x = i / x end
           return y
         end return f",
    ),
    (
        "fl_cmp",
        "local function f(n, a, b, c, t, g)
           local x, y, z = a + 0.25, 0.0, 1
           for i = 1, n do
             x = x + 0.75
             if x > 3 then x = x - 3.5 elseif x < -2.0 then x = -x end
             if x == 1.25 then y = y + 1.0 end
             if x <= y then z = z + 1 end
             if y >= 2.5 then y = 0.5 end
             if x ~= x then y = y + 100 end
           end
           return z + y
         end return f",
    ),
    (
        "fl_for_up",
        "local function f(n, a, b, c, t, g)
           local s = 0.0
           for x = a + 0.5, b + 10, 1.5 do s = s + x end
           return s
         end return f",
    ),
    (
        "fl_for_down",
        "local function f(n, a, b, c, t, g)
           local s = 0.0
           for x = 10.0, a - 0.5, -0.75 do s = s - x * 2 end
           return s
         end return f",
    ),
    (
        "fl_for_n",
        "local function f(n, a, b, c, t, g)
           local s = 0.0
           for x = 0.5, n, 0.25 do s = s + x / 3 end
           return s
         end return f",
    ),
    (
        "fl_fallbacks",
        "local function f(n, a, b, c, t, g)
           local x, y = a + 0.5, 1.0
           for i = 1, n do
             x = x + i * 0.5
             y = y + x % 3.0
             y = y + (x // 2)
             y = y + x ^ 2
             if y > 1e6 then y = 0.5 end
             x = x - 1
           end
           return y
         end return f",
    ),
    (
        "fl_special",
        "local function f(n, a, b, c, t, g)
           local nan, inf, nz = 0.0 / 0.0, 1e308 * 10, -0.0
           local x, y = nz, 1.0
           for i = 1, n do
             x = x + nz
             y = y * inf
             if y ~= y then y = 1.0 end
             if x == nan then x = 2.0 end
             x = x / (i - 3)
             y = y / inf + nan * 0
           end
           return x
         end return f",
    ),
    (
        "len_get",
        "local function f(n, a, b, c, t, g)
           for i = 1, n do
             a = a + i
             b = #t
             c = c + b % 5
             a = a + t[i]
             if a > 100 then a = a - 100 end
           end
           return a
         end return f",
    ),
    (
        "call",
        "local function f(n, a, b, c, t, g)
           for i = 1, n do a = a + i; b = g(a); a = a + b; c = c ~ a end
           return a
         end return f",
    ),
    (
        "set_get",
        "local function f(n, a, b, c, t, g)
           for i = 1, n do
             t[i] = a
             a = a + t[i % 3 + 1] * 2
             b = b + i
             if b > 50 then b = b - 50 end
           end
           return a
         end return f",
    ),
    (
        "pow_concat",
        "local function f(n, a, b, c, t, g)
           for i = 1, n do
             a = a + i
             b = a ^ 2
             c = a .. i
             a = a - 1
           end
           return a
         end return f",
    ),
    (
        "nested_call",
        "local function f(n, a, b, c, t, g)
           for i = 1, n do
             for j = 1, i do
               a = a + j * i
               if j % 2 == 0 then b = g(j) end
               c = c + a
             end
             a = a - i
           end
           return a
         end return f",
    ),
    (
        "newtable_while",
        "local function f(n, a, b, c, t, g)
           local k = 0
           while k < n do
             k = k + 1
             t = {}
             a = a + k
             if a > 30 then a = a - 30; b = g(a, k) end
           end
           return a
         end return f",
    ),
];

/// Coverage counters of [`compare_calls`].
#[derive(Default, Debug)]
struct Cov {
    region_events: u64,
    tier1_events: u64,
    helper_calls: u64,
    mutations: u64,
    moves: u64,
}

fn compare_calls(name: &str, opt: &str, chaos: bool, obsv: u32) -> Cov {
    let mut cov = Cov::default();
    let src = CALL_PROGS.iter().find(|(n, _)| *n == name).unwrap().1;
    let f = func_of(src);
    let mut seed = 0x1357_9bdf_2468_ace0u64 ^ ((name.len() as u64) << 40) ^ u64::from(chaos);
    let cases = if cfg!(miri) { 4 } else { 160 };
    let mut regions_seen = 0;
    for case in 0..cases {
        let mut m = Mem::new();
        m.set_k(&f.consts);
        m.setup_ci(
            (rnd_state(&mut seed) as u32 % 3) | ci::CIST_FRESH,
            i32::from(rnd_state(&mut seed).is_multiple_of(11)),
        );
        // (a `RETURN` of a fresh frame leaves with `Returned`; its caller `CallInfo` must be readable memory)
        Mem::put(&mut m.ci, ci::PREVIOUS, dummy_ci());
        m.set_reg(0, LUA_VNUMINT, rnd_state(&mut seed) % 9);
        for r in 1..4 {
            m.set_reg(r, LUA_VNUMINT, (rnd_state(&mut seed) % 21).wrapping_sub(10));
        }
        // t (4) and g (5): nil, integer or float: every value takes a different path through the table / call templates
        for r in 4..6 {
            match rnd_state(&mut seed) % 4 {
                0 | 1 => m.set_reg(r, LUA_VNIL, 0),
                2 => m.set_reg(r, LUA_VNUMINT, rnd_state(&mut seed) % 5),
                _ => m.set_reg(r, LUA_VNUMFLT, 2.5f64.to_bits()),
            }
        }
        if rnd_state(&mut seed).is_multiple_of(7) {
            m.set_reg(2, LUA_VNUMFLT, 1.5f64.to_bits());
        }
        m.cnt = if rnd_state(&mut seed).is_multiple_of(3) {
            (rnd_state(&mut seed) % 60) as i32
        } else {
            i32::MAX
        };
        m.irq[0] = u8::from(rnd_state(&mut seed).is_multiple_of(25));
        let mseed = rnd_state(&mut seed);
        if std::env::var_os("T12_TRACE").is_some() {
            eprintln!(
                "  regs: {:?} cnt {} irq {} ci {:?}",
                &m.frame[PRE * 2..PRE * 2 + 12],
                m.cnt,
                m.irq[0],
                Mem::get::<u32>(&m.ci, ci::CALLSTATUS)
            );
        }
        if std::env::var_os("T12_TRACE").is_some() {
            eprintln!("{name} `{opt}` chaos={chaos} case {case}");
        }
        let mut m1 = m.clone();
        let mut m2 = m.clone();
        let plain = tier1_calls(&f, m1.irq.as_ptr());
        let low = region_calls(&f, m2.irq.as_ptr(), opt, obsv);
        let range = {
            let nests = scan_nests(&f.code);
            (nests[0].0, nests.iter().map(|n| n.1).max().unwrap())
        };
        if !low.points.is_empty() {
            regions_seen += 1;
        }
        if std::env::var_os("T12_TRACE").is_some() {
            eprintln!("  tier1 run");
        }
        let t = trace(
            &plain,
            &t1_stops(&plain, range),
            &mut m1,
            &mut Model::new(mseed, chaos),
            None,
        );
        if std::env::var_os("T12_TRACE").is_some() {
            eprintln!("  region run");
        }
        let r = trace(
            &low,
            &region_stops(&low, &plain, range),
            &mut m2,
            &mut Model::new(mseed, chaos),
            None,
        );
        assert!(
            !matches!(t.end, Until::Stop(Stop::Unsupported | Stop::OutOfSteps)),
            "{name}: tier 1 did not finish: {:?}",
            t.end
        );
        let mut t = t;
        if low.points.is_empty() {
            t.events.clear(); // (the generator refused the region: the program is tier 1; compare the end state only)
        }
        diff_events(name, opt, case, &t, &r);
        cov.region_events += r.events.iter().filter(|e| e.region).count() as u64;
        cov.tier1_events += r.events.iter().filter(|e| !e.region).count() as u64;
        cov.helper_calls += r.calls;
        cov.mutations += r.log.iter().filter(|l| l.contains(":= (")).count() as u64;
        cov.moves += r.log.iter().filter(|l| l.contains("stack moved")).count() as u64;
    }
    let refusing = std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"));
    if !rn_core::jit_layout::LUA54 && !refusing && obsv == 0 {
        assert!(regions_seen > 0, "{name}: no region was generated");
    }
    cov
}

/// The float programs really produce XMM code under some entry speculation (otherwise the comparison tests would
/// pass vacuously).
#[test]
fn float_programs_generate_xmm_region_code() {
    use rn_jit::ir::Op;
    if rn_core::jit_layout::LUA54
        || std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"))
    {
        return;
    }
    let m = Mem::new();
    for (name, src) in CALL_PROGS.iter().filter(|(n, _)| n.starts_with("fl_")) {
        let f = func_of(src);
        let mut best = 0usize;
        for v in 0..4 {
            let low = region_calls(&f, m.irq.as_ptr(), "", v);
            let n = low
                .prog
                .ops
                .iter()
                .filter(|o| {
                    matches!(
                        o,
                        Op::FBin { .. }
                            | Op::FBinM { .. }
                            | Op::FBrX { .. }
                            | Op::MovF(..)
                            | Op::FNeg(_)
                    )
                })
                .count();
            if std::env::var_os("T4B_COUNT").is_some() {
                eprintln!("{name} variant {v}: {n} xmm ops");
            }
            best = best.max(n);
        }
        assert!(best > 0, "{name}: no XMM region code under any speculation");
    }
}

#[test]
fn regions_with_helper_calls_match_tier1_at_every_pc_and_exit() {
    let mut total = Cov::default();
    for (name, _) in CALL_PROGS {
        for opt in [
            "",
            "-ra",
            "+randra=3",
            "+randra=11",
            "+flushall",
            "+verify",
            "+verify,+flushall",
        ] {
            for chaos in [false, true] {
                // float programs: several entry speculations (the others use all-integer frames only)
                let variants: &[u32] = if name.starts_with("fl_") {
                    &[0, 1, 2, 3]
                } else {
                    &[0]
                };
                for &v in variants {
                    let c = compare_calls(name, opt, chaos, v);
                    total.region_events += c.region_events;
                    total.tier1_events += c.tier1_events;
                    total.helper_calls += c.helper_calls;
                    total.mutations += c.mutations;
                    total.moves += c.moves;
                }
            }
        }
    }
    eprintln!("t15_core_emu helper-call coverage: {total:?}");
    let refusing = std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"));
    if !cfg!(miri) && !rn_core::jit_layout::LUA54 && !refusing {
        assert!(total.region_events > 2000, "{total:?}");
        assert!(total.tier1_events > 500, "{total:?}");
        assert!(total.helper_calls > 1000, "{total:?}");
        assert!(total.mutations > 100, "{total:?}");
        assert!(total.moves > 100, "{total:?}");
    }
}

// ------------------------------------------------------------------------------------------------ T5: tables
//
// Inline table accesses (`tables.rs`) on synthetic tables: a pool of hand-built `Table`s (array part with random
// element tags, hash part with real interned short-string keys in main positions and collision chains, random `marked`
// colours and metatable pointers) in memory that both runs start from; the pool is part of every compared event (so
// every store, and every store that must NOT happen, is checked at every region pc). The helper model answers the
// slow paths of both runs identically; the number of helper calls is compared, so a fast path that decides differently
// from tier 1's (bounds, nil, metatable, barrier, non-existing node) shows up as a different call count or frame.

use rn_core::host::CompilerHost;
use rn_core::jit_layout::{node as jn, table as jt};
use rn_core::state::LuaState;
use rn_core::value::{ctb, LUA_VEMPTY, LUA_VFALSE, LUA_VSHRSTR, LUA_VTABLE};

struct Pool {
    /// (`Cell`s: raw pointers into the pool stay valid for writes while the pool is read and restored through `&self`)
    buf: Box<[std::cell::Cell<u64>]>,
    used: usize,
    saved: Vec<u64>,
}

impl Pool {
    fn new() -> Pool {
        Pool {
            buf: (0..6144).map(|_| std::cell::Cell::new(0)).collect(),
            used: 0,
            saved: Vec::new(),
        }
    }
    fn alloc(&mut self, bytes: usize) -> *mut u8 {
        let w = bytes.div_ceil(8).next_multiple_of(2);
        let at = self.used;
        self.used += w;
        assert!(self.used <= self.buf.len(), "pool exhausted");
        self.buf
            .as_ptr()
            .cast::<u8>()
            .cast_mut()
            .wrapping_add(at * 8)
    }
    fn snapshot(&self) -> Vec<u64> {
        self.buf[..self.used]
            .iter()
            .map(std::cell::Cell::get)
            .collect()
    }
    fn save(&mut self) {
        self.saved = self.snapshot();
    }
    fn restore(&mut self) {
        for (c, &v) in self.buf.iter().zip(&self.saved) {
            c.set(v);
        }
    }
}

fn wr<T: Copy>(p: *mut u8, off: usize, v: T) {
    // SAFETY: callers write inside blocks returned by `Pool::alloc` (the pool outlives them).
    unsafe { p.add(off).cast::<T>().write_unaligned(v) }
}
fn rd<T: Copy>(p: *mut u8, off: usize) -> T {
    // SAFETY: as in `wr`.
    unsafe { p.add(off).cast::<T>().read_unaligned() }
}

/// An interned short-string constant: its object address and hash.
#[derive(Clone, Copy)]
struct StrK {
    ptr: u64,
    hash: u32,
}

/// A random value `(tag, payload)`; `tabs` are the tables a `TABLE` value may point to. `clean`: numbers and nil only.
fn rand_val(r: &mut u64, tabs: &[u64], clean: bool) -> (u8, u64) {
    let x = rnd_state(r);
    let ints = [0u64, 1, 2, 3, 7, 100, u64::MAX, 1 << 40, 9];
    let flts = [0.5f64, -2.0, 3.0, 1e300, -0.0, 7.25, 0.0];
    match x % 20 {
        0..=7 => (LUA_VNUMINT, ints[(x >> 8) as usize % ints.len()]),
        8..=13 => (LUA_VNUMFLT, flts[(x >> 8) as usize % flts.len()].to_bits()),
        14 | 15 => (if x & 0x100 == 0 { LUA_VNIL } else { LUA_VEMPTY }, 0),
        _ if clean => (LUA_VNUMINT, x >> 40 & 15),
        16 => (LUA_VTRUE, 0),
        17 => (LUA_VFALSE, 0),
        _ if !tabs.is_empty() => (ctb(LUA_VTABLE), tabs[(x >> 8) as usize % tabs.len()]),
        _ => (LUA_VNUMINT, 5),
    }
}

fn mark_byte(r: &mut u64) -> u8 {
    [0x08u8, 0x10, 0x20, 0x20, 0x00, 0x28][rnd_state(r) as usize % 6]
}

/// Builds a table in the pool; `tabs` lists tables built so far (values of kind `TABLE` point to them).
fn mk_table(pool: &mut Pool, r: &mut u64, keys: &[StrK], tabs: &[u64], clean: bool) -> u64 {
    let hdr = pool.alloc(48);
    let n = rnd_state(r) as usize % 7;
    let lsz = (rnd_state(r) % 4) as u8;
    wr(hdr, jt::MARKED, mark_byte(r));
    wr(hdr, jt::FLAGS, 0u8);
    wr(hdr, jt::LSIZENODE, lsz);
    wr(hdr, jt::ASIZE, n as u32);
    if n == 0 {
        wr(hdr, jt::ARRAY, 0usize);
    } else {
        let base = pool.alloc(n * 8 + 8 + n);
        let arr = base.wrapping_add(n * 8);
        wr(arr, 0, n as u32); // length hint
        for k in 0..n {
            let (tag, pay) = rand_val(r, tabs, clean);
            wr(arr, jt::ARRAY_TAG_BASE + k, tag);
            wr(
                arr,
                0usize.wrapping_sub(jt::ARRAY_PAYLOAD_STRIDE * (k + 1)),
                pay,
            );
        }
        wr(hdr, jt::ARRAY, arr as usize);
    }
    let nn = 1usize << lsz;
    let nodes = pool.alloc(nn * jn::SIZE);
    let mut hash_of = vec![0u32; nn];
    for &k in keys {
        if rnd_state(r) % 10 >= 7 {
            continue;
        }
        let mp = k.hash as usize & (nn - 1);
        let used = |i: usize| rd::<u8>(nodes, i * jn::SIZE + jn::KEY_TT) != 0;
        let target = if !used(mp) {
            Some(mp)
        } else if hash_of[mp] as usize & (nn - 1) == mp {
            // a free node, chained after the last node of the chain starting at `mp`
            let free = (0..nn).rev().find(|&i| !used(i));
            if let Some(f) = free {
                let mut last = mp;
                loop {
                    let nx = rd::<i32>(nodes, last * jn::SIZE + jn::NEXT);
                    if nx == 0 {
                        break;
                    }
                    last = (last as i64 + i64::from(nx)) as usize;
                }
                wr(
                    nodes,
                    last * jn::SIZE + jn::NEXT,
                    (f as i64 - last as i64) as i32,
                );
            }
            free
        } else {
            None
        };
        if let Some(i) = target {
            let (mut tag, mut pay) = rand_val(r, tabs, clean);
            if rnd_state(r).is_multiple_of(8) {
                tag = LUA_VNIL; // a dead key
                pay = 0;
            }
            let o = i * jn::SIZE;
            wr(nodes, o + jn::VAL_TT, tag);
            wr(nodes, o + jn::VAL, pay);
            wr(nodes, o + jn::KEY_TT, ctb(LUA_VSHRSTR));
            wr(nodes, o + jn::KEY, k.ptr);
            hash_of[i] = k.hash;
        }
    }
    wr(hdr, jt::NODE, nodes as usize);
    wr(
        hdr,
        jt::METATABLE,
        if rnd_state(r) % 10 < 3 {
            hdr as usize
        } else {
            0usize
        },
    );
    hdr as u64
}

/// Like [`Func`] with string constants.
fn func_of_str(src: &str) -> (Func, LuaState, Vec<StrK>) {
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
    let f: &ProtoData<u32> = &p.protos[0];
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let mut keys = Vec::new();
    let consts =
        f.k.iter()
            .map(|c| match c {
                Const::Int(i) => Value::int(*i),
                Const::Float(x) => Value::float(*x),
                Const::Str(i) => {
                    let v = st
                        .new_str_value(CompilerHost::bytes(&host, *i))
                        .expect("string");
                    let s = v.as_string().expect("string");
                    keys.push(StrK {
                        ptr: s.as_ptr() as u64,
                        hash: s.hash(),
                    });
                    v
                }
                _ => panic!("unsupported constant"),
            })
            .collect();
    (
        Func {
            code: f.code.clone(),
            consts,
            max_stack: u32::from(f.max_stack_size),
        },
        st,
        keys,
    )
}

const TAB_PROGS: &[(&str, &str)] = &[
    (
        "arr_rw",
        "local function f(n, a, b, c, t, u)
           for i = 1, n do
             a = a + (t[i] or 1)
             t[i] = a
             b = t[2] and 1 or 2
             t[3] = b
             u[i] = t[i]
             c = c + i
           end
           return a
         end return f",
    ),
    (
        "fld_rw",
        "local function f(n, a, b, c, t, u)
           for i = 1, n do
             a = a + (t.x or 1)
             t.x = a
             t.y = t.y
             u.z = t.x
             b = u.w
             c = c + i
           end
           return a
         end return f",
    ),
    (
        "mix_flt",
        "local function f(n, a, b, c, t, u)
           local s = 0.5
           for i = 1, n do
             s = s + (t[i] or 0.25) * 0.5
             u[i] = s
             t.x = s
             a = a + 1
           end
           return s
         end return f",
    ),
    (
        "nested",
        "local function f(n, a, b, c, t, u)
           for i = 1, n do
             local r = t[i]
             if r then a = a + (r[1] or 0); r[2] = i; r.y = a end
             u[i % 3 + 1] = r
           end
           return a
         end return f",
    ),
    (
        "geti_seti",
        "local function f(n, a, b, c, t, u)
           for i = 1, n do
             a = a + (t[1] or 0) + (t[4] or 0)
             t[2] = a
             u[1] = t[3]
             t[5] = i
             u.x = t[6]
           end
           return a
         end return f",
    ),
];

#[derive(Default, Debug)]
struct TabCov {
    region_events: u64,
    tier1_events: u64,
    helper_calls: u64,
}

fn region_tab(f: &Func, irq: *const u8, opt: &str, obsv: u32, profv: u32) -> Lowered {
    let nests = scan_nests(&f.code);
    assert!(!nests.is_empty(), "no loop");
    let plans: Vec<NestPlan> = nests
        .iter()
        .enumerate()
        .map(|(i, &(head, end))| NestPlan {
            nest: i as u32,
            head,
            end,
        })
        .collect();
    // slots 4 and 5 (`t`, `u`) are tables; the others integers (obsv 0) or floats from slot 6 (obsv 1)
    let tag = |s: usize| {
        if s == 4 || s == 5 {
            ctb(LUA_VTABLE)
        } else if obsv == 1 && s >= 6 {
            LUA_VNUMFLT
        } else {
            LUA_VNUMINT
        }
    };
    let obs = vec![(0..f.max_stack as usize).map(tag).collect(); plans.len()];
    let ops: Vec<Option<rn_core::ver::active::OpCode>> = f
        .code
        .iter()
        .map(|&w| rn_core::instruction::Instruction(w).opcode())
        .collect();
    // the profile of the loads: none, all integers, all floats, tables, or by pc parity
    let prof: Vec<u8> = ops
        .iter()
        .enumerate()
        .map(|(pc, op)| {
            use rn_core::ver::active::OpCode as O;
            if !matches!(op, Some(O::Gettable | O::Geti | O::Getfield)) {
                return 0xFF;
            }
            match profv {
                1 => LUA_VNUMINT,
                2 => LUA_VNUMFLT,
                3 => ctb(LUA_VTABLE),
                4 if pc % 2 == 0 => LUA_VNUMINT,
                4 => LUA_VNUMFLT,
                _ => 0xFF,
            }
        })
        .collect();
    let meta = FuncMeta {
        max_stack: f.max_stack,
        escaped: SlotSet::EMPTY,
        obs,
        prof,
        intr: Vec::new(),
        opt: OptFlags::parse(&format!("{opt},+force")),
    };
    lower_with_regions(&f.code, calls_cfg(f, irq), &plans, Some(&meta)).expect("lowers")
}

fn compare_tables(
    name: &str,
    opt: &str,
    chaos: bool,
    obsv: u32,
    profv: u32,
    clean: bool,
) -> TabCov {
    use rn_jit::ir::Op;
    let mut cov = TabCov::default();
    let src = TAB_PROGS.iter().find(|(n, _)| *n == name).unwrap().1;
    let (f, _st, keys) = func_of_str(src);
    let mut seed = 0x2468_ace0_1357_9bdfu64 ^ ((name.len() as u64) << 36) ^ u64::from(chaos);
    let cases = if cfg!(miri) { 3 } else { 120 };
    let mut saw_inline = false;
    for case in 0..cases {
        let mut pool = Pool::new();
        let mut tabs: Vec<u64> = Vec::new();
        for _ in 0..4 {
            let t = mk_table(&mut pool, &mut seed, &keys, &tabs, clean);
            tabs.push(t);
        }
        let mut m = Mem::new();
        m.set_k(&f.consts);
        m.setup_ci(
            (rnd_state(&mut seed) as u32 % 3) | ci::CIST_FRESH,
            i32::from(rnd_state(&mut seed).is_multiple_of(11)),
        );
        Mem::put(&mut m.ci, ci::PREVIOUS, dummy_ci());
        m.set_reg(0, LUA_VNUMINT, rnd_state(&mut seed) % 9);
        for r in 1..4 {
            if obsv == 1 && rnd_state(&mut seed).is_multiple_of(2) {
                m.set_reg(r, LUA_VNUMFLT, 1.5f64.to_bits());
            } else {
                m.set_reg(r, LUA_VNUMINT, (rnd_state(&mut seed) % 21).wrapping_sub(10));
            }
        }
        for r in 4..6 {
            // mostly tables; sometimes nil / a number (entry guard failure, run-time table check)
            match rnd_state(&mut seed) % 12 {
                0 => m.set_reg(r, LUA_VNIL, 0),
                1 => m.set_reg(r, LUA_VNUMINT, 3),
                _ => m.set_reg(
                    r,
                    ctb(LUA_VTABLE),
                    tabs[(rnd_state(&mut seed) % 4) as usize],
                ),
            }
        }
        m.cnt = if rnd_state(&mut seed).is_multiple_of(3) {
            (rnd_state(&mut seed) % 80) as i32
        } else {
            i32::MAX
        };
        m.irq[0] = u8::from(rnd_state(&mut seed).is_multiple_of(25));
        let mseed = rnd_state(&mut seed);
        pool.save();
        let mut m1 = m.clone();
        let mut m2 = m.clone();
        let plain = tier1_calls(&f, m1.irq.as_ptr());
        let low = region_tab(&f, m2.irq.as_ptr(), opt, obsv, profv);
        let range = {
            let nests = scan_nests(&f.code);
            (nests[0].0, nests.iter().map(|n| n.1).max().unwrap())
        };
        saw_inline |= low
            .prog
            .ops
            .iter()
            .any(|o| matches!(o, Op::LdI { .. } | Op::StI { .. } | Op::BrMi { .. }))
            || low.prog.ops.len() > plain.prog.ops.len() + 200;
        pool.restore();
        let t = trace(
            &plain,
            &t1_stops(&plain, range),
            &mut m1,
            &mut Model::new(mseed, chaos),
            Some(&pool),
        );
        pool.restore();
        let r = trace(
            &low,
            &region_stops(&low, &plain, range),
            &mut m2,
            &mut Model::new(mseed, chaos),
            Some(&pool),
        );
        assert!(
            !matches!(t.end, Until::Stop(Stop::Unsupported | Stop::OutOfSteps)),
            "{name}: tier 1 did not finish: {:?}",
            t.end
        );
        let mut t = t;
        if low.points.is_empty() {
            t.events.clear();
        }
        diff_events(&format!("{name} prof{profv} obs{obsv}"), opt, case, &t, &r);
        cov.region_events += r.events.iter().filter(|e| e.region).count() as u64;
        cov.tier1_events += r.events.iter().filter(|e| !e.region).count() as u64;
        cov.helper_calls += r.calls;
    }
    let refusing = std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"));
    if !rn_core::jit_layout::LUA54 && !refusing && opt != "-tables" {
        assert!(saw_inline, "{name}: no inline table access was generated");
    }
    cov
}

#[test]
fn inline_table_accesses_match_tier1_at_every_pc_on_synthetic_tables() {
    let mut total = TabCov::default();
    for (name, _) in TAB_PROGS {
        for opt in [
            "",
            "-ra",
            "+randra=3",
            "+randra=11",
            "+flushall",
            "+verify",
            "-tables",
        ] {
            for chaos in [false, true] {
                for (obsv, profv) in [(0, 0), (0, 1), (1, 2), (0, 3), (1, 4)] {
                    for clean in [true, false] {
                        let c = compare_tables(name, opt, chaos, obsv, profv, clean);
                        total.region_events += c.region_events;
                        total.tier1_events += c.tier1_events;
                        total.helper_calls += c.helper_calls;
                    }
                }
            }
        }
    }
    eprintln!("t15_core_emu table coverage: {total:?}");
    let refusing = std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"));
    if !cfg!(miri) && !rn_core::jit_layout::LUA54 && !refusing {
        assert!(total.region_events > 5000, "{total:?}");
        assert!(total.helper_calls > 2000, "{total:?}");
    }
}

// ------------------------------------------------------------------------------------------------ T6: intrinsics
//
// Inline `math` calls (`codegen/calls.rs`). The callee is a parameter holding a light C function whose payload is the
// registered address of the function under test. The model answers every call of a registered function with numeric
// operands with the real result and without consuming random decisions (`Model::try_math`), so the tier-1 run (always
// through the helper) and the region run (inline, or through the helper after a failed guard) stay in step. The
// scenarios break one guard at a time (hook, interrupt, stack room, spare CallInfo, callee identity/tag): the region run
// must then call the helper exactly as often as tier 1 does (`math_calls`), in the normal scenario it must call it
// less. Everything else is compared at every region pc as in the other tests: frame, budget, CallInfo (`savedpc`),
// thread (`top`).

use rn_core::value::LUA_VLCF;

const IN_PROGS_EMU: &[(&str, u8, &str)] = &[
    (
        "in_sqrt_flt",
        0,
        "local function f(n, a, b, c, t, g)
           local x, s = a + 0.5, 0.0
           for i = 1, n do x = x * 1.25 + 0.5; s = s + g(x) end
           return s
         end return f",
    ),
    (
        "in_sqrt_int",
        0,
        "local function f(n, a, b, c, t, g)
           local s = 0.0
           for i = 1, n do s = s + g(i + a) end
           return s
         end return f",
    ),
    (
        "in_abs_int",
        1,
        "local function f(n, a, b, c, t, g)
           local s = 0
           for i = 1, n do s = s + g(a - i * 3) end
           return s
         end return f",
    ),
    (
        "in_abs_flt",
        1,
        "local function f(n, a, b, c, t, g)
           local x, s = a + 0.5, 0.0
           for i = 1, n do x = x * -1.25 + 0.5; s = s + g(x) end
           return s
         end return f",
    ),
    (
        "in_floor_flt",
        2,
        "local function f(n, a, b, c, t, g)
           local x, s = a + 0.5, 0
           for i = 1, n do x = x * 3.0e5 - 3.5; s = s + g(x) end
           return s
         end return f",
    ),
    (
        "in_floor_int",
        2,
        "local function f(n, a, b, c, t, g)
           local s = 0
           for i = 1, n do s = s + g(i - a) end
           return s
         end return f",
    ),
    (
        "in_ceil_flt",
        3,
        "local function f(n, a, b, c, t, g)
           local x, s = a + 0.5, 0
           for i = 1, n do x = x * -3.0e5 + 3.5; s = s + g(x) end
           return s
         end return f",
    ),
    (
        "in_min_flt",
        4,
        "local function f(n, a, b, c, t, g)
           local x, s = a + 0.5, 0.0
           for i = 1, n do x = x * 1.5 - 2.25; s = s + g(x, 1.5) end
           return s
         end return f",
    ),
    (
        "in_max_flt",
        5,
        "local function f(n, a, b, c, t, g)
           local x, s = a + 0.5, 0.0
           for i = 1, n do x = x * -1.5 + 2.25; s = s + g(1.5, x) end
           return s
         end return f",
    ),
    (
        "in_min_int",
        4,
        "local function f(n, a, b, c, t, g)
           local s = 0
           for i = 1, n do s = s + g(i, a) end
           return s
         end return f",
    ),
    (
        "in_max_int",
        5,
        "local function f(n, a, b, c, t, g)
           local s = 0
           for i = 1, n do s = s + g(a, i * 2) end
           return s
         end return f",
    ),
    (
        "in_sin_flt",
        6,
        "local function f(n, a, b, c, t, g)
           local x, s = a + 0.5, 0.0
           for i = 1, n do x = x * 1.25 + 0.5; s = s + g(x) end
           return s
         end return f",
    ),
    (
        "in_cos_int",
        7,
        "local function f(n, a, b, c, t, g)
           local s = 0.0
           for i = 1, n do s = s + g(i + a) end
           return s
         end return f",
    ),
    (
        "in_exp_flt",
        8,
        "local function f(n, a, b, c, t, g)
           local x, s = a + 0.5, 0.0
           for i = 1, n do x = x * 0.5 + 0.25; s = s + g(x) end
           return s
         end return f",
    ),
    (
        "in_log_flt",
        9,
        "local function f(n, a, b, c, t, g)
           local x, s = a + 0.5, 0.0
           for i = 1, n do x = x * 1.25 + 0.5; s = s + g(x) end
           return s
         end return f",
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scn {
    Normal,
    Hook,
    Irq,
    StackFull,
    NoSpareCi,
    WrongPayload,
    WrongTag,
}

const SCENARIOS: [Scn; 7] = [
    Scn::Normal,
    Scn::Hook,
    Scn::Irq,
    Scn::StackFull,
    Scn::NoSpareCi,
    Scn::WrongPayload,
    Scn::WrongTag,
];

fn register_intrinsics() {
    // the same values `rn_jit::register` installs (idempotent)
    rn_core::jit_hooks::set_intrinsics(rn_std::lib_math::jit_builtins());
}

fn region_intr(f: &Func, irq: *const u8, opt: &str, obsv: u32, kind: u8) -> Lowered {
    use rn_core::ver::active::OpCode as O;
    let nests = scan_nests(&f.code);
    assert!(!nests.is_empty(), "no loop");
    let plans: Vec<NestPlan> = nests
        .iter()
        .enumerate()
        .map(|(i, &(head, end))| NestPlan {
            nest: i as u32,
            head,
            end,
        })
        .collect();
    let mut obs = obs_for(obsv, f.max_stack as usize, plans.len());
    for o in &mut obs {
        o[5] = LUA_VLCF; // the callee parameter
    }
    let calls: Vec<u32> = f
        .code
        .iter()
        .enumerate()
        .filter(|(_, &w)| rn_core::instruction::Instruction(w).opcode() == Some(O::Call))
        .map(|(pc, _)| pc as u32)
        .collect();
    let mut prof = vec![0xFF; f.code.len()];
    for &pc in &calls {
        // the result type the warm-up profile would have recorded
        prof[pc as usize] = match kind {
            2 | 3 => LUA_VNUMINT,
            4 | 5 if obsv != 1 => LUA_VNUMINT,
            _ => LUA_VNUMFLT,
        };
    }
    let meta = FuncMeta {
        max_stack: f.max_stack,
        escaped: SlotSet::EMPTY,
        obs,
        prof,
        intr: calls
            .iter()
            .map(|&pc| {
                (
                    pc,
                    rn_jit::t15::intrinsics::Intr::from_index(kind).expect("kind"),
                )
            })
            .collect(),
        opt: OptFlags::parse(&format!("{opt},+force")),
    };
    lower_with_regions(&f.code, calls_cfg(f, irq), &plans, Some(&meta)).expect("lowers")
}

#[derive(Default, Debug)]
struct IntrCov {
    fast: u64,
    slow: u64,
    region_events: u64,
}

fn compare_intr(
    name: &str,
    kind: u8,
    opt: &str,
    obsv: u32,
    scn: Scn,
    src: &str,
    cov: &mut IntrCov,
) {
    use rn_core::jit_hooks::thread as thr;
    register_intrinsics();
    let f = func_of(src);
    let addr = rn_core::jit_hooks::intrinsic_addr(kind) as u64;
    assert_ne!(addr, 0);
    let mut seed = 0x0bad_cafe_1357_9bdfu64
        ^ ((name.len() as u64) << 40)
        ^ (u64::from(kind) << 8)
        ^ scn as u64;
    let cases = if cfg!(miri) { 3 } else { 60 };
    for case in 0..cases {
        let mut m = Mem::new();
        m.set_k(&f.consts);
        m.setup_ci(
            (rnd_state(&mut seed) as u32 % 3) | ci::CIST_FRESH,
            i32::from(rnd_state(&mut seed).is_multiple_of(11)),
        );
        Mem::put(&mut m.ci, ci::PREVIOUS, dummy_ci());
        Mem::put(&mut m.ci, ci::NEXT, 0x7100usize);
        Mem::put(&mut m.thread, thr::STACK_SIZE, 100_000usize);
        // n (slot 0), a (1): small integers, sometimes a float; the float programs use the "all float" observation
        m.set_reg(0, LUA_VNUMINT, 1 + rnd_state(&mut seed) % 8);
        m.set_reg(1, LUA_VNUMINT, (rnd_state(&mut seed) % 21).wrapping_sub(10));
        for r in 2..5 {
            m.set_reg(r, LUA_VNUMINT, (rnd_state(&mut seed) % 21).wrapping_sub(10));
        }
        m.set_reg(5, LUA_VLCF, addr);
        m.cnt = if rnd_state(&mut seed).is_multiple_of(4) {
            (rnd_state(&mut seed) % 60) as i32
        } else {
            i32::MAX
        };
        m.irq[0] = 0;
        match scn {
            Scn::Normal => {}
            Scn::Hook => Mem::put(&mut m.thread, thr::HOOKMASK, 1u8),
            Scn::Irq => m.irq[0] = 1,
            Scn::StackFull => Mem::put(&mut m.thread, thr::STACK_SIZE, 30usize),
            Scn::NoSpareCi => Mem::put(&mut m.ci, ci::NEXT, 0usize),
            Scn::WrongPayload => m.set_reg(5, LUA_VLCF, addr.wrapping_add(8)),
            Scn::WrongTag => m.set_reg(5, rn_core::value::LUA_VLCFC, addr),
        }
        let mseed = rnd_state(&mut seed);
        let mut m1 = m.clone();
        let mut m2 = m.clone();
        let plain = tier1_calls(&f, m1.irq.as_ptr());
        let low = region_intr(&f, m2.irq.as_ptr(), opt, obsv, kind);
        let range = {
            let nests = scan_nests(&f.code);
            (nests[0].0, nests.iter().map(|n| n.1).max().unwrap())
        };
        let mut mt = Model::new(mseed, false);
        let t = trace(&plain, &t1_stops(&plain, range), &mut m1, &mut mt, None);
        let mut mr = Model::new(mseed, false);
        let r = trace(
            &low,
            &region_stops(&low, &plain, range),
            &mut m2,
            &mut mr,
            None,
        );
        assert!(
            !matches!(t.end, Until::Stop(Stop::Unsupported | Stop::OutOfSteps)),
            "{name}: tier 1 did not finish: {:?}",
            t.end
        );
        let mut t = t;
        if low.points.is_empty() {
            t.events.clear();
        }
        let label = format!("{name} {scn:?} obs{obsv}");
        diff_events(&label, opt, case, &t, &r);
        cov.region_events += r.events.iter().filter(|e| e.region).count() as u64;
        if low.points.is_empty() {
            continue;
        }
        match scn {
            Scn::Normal => {
                assert!(
                    mr.math_calls <= mt.math_calls,
                    "{label}: {} > {}",
                    mr.math_calls,
                    mt.math_calls
                );
                cov.fast += mt.math_calls - mr.math_calls;
            }
            // a guard that must fail: every call goes through the helper, like tier 1's (the floor / ceil of values
            // that are not integers go through the helper in both runs as well)
            _ => {
                assert_eq!(
                    mr.math_calls, mt.math_calls,
                    "{label}: case {case}: the inline form ran past a failed guard"
                );
                cov.slow += mt.math_calls;
            }
        }
    }
}

#[test]
fn intrinsic_calls_match_tier1_at_every_pc() {
    let mut cov = IntrCov::default();
    for &(name, kind, src) in IN_PROGS_EMU {
        for opt in ["", "-ra", "+randra=3", "+flushall", "+verify"] {
            for obsv in [0, 1, 2, 3] {
                for scn in SCENARIOS {
                    compare_intr(name, kind, opt, obsv, scn, src, &mut cov);
                }
            }
        }
    }
    eprintln!("t15_core_emu intrinsic coverage: {cov:?}");
    let refusing = std::env::var("RUSTYNAIL_JIT_MUT").is_ok_and(|v| v.contains("bad_fall_targets"));
    if !cfg!(miri) && !rn_core::jit_layout::LUA54 && !refusing {
        assert!(cov.fast > 200, "{cov:?}");
        assert!(cov.slow > 200, "{cov:?}");
        assert!(cov.region_events > 2000, "{cov:?}");
    }
}

// ------------------------------------------------------------------------------------------------ T7: failure pads

use rn_jit::t15::runtime::{lower_with_regions_ex, FeedbackSpec, NestPolicy, PolicyCtl};

const FAKE_EVENT: u64 = 0xE7E7_0001;

/// Answers the event helper: records the call, re-arms the countdown like the real helper does, returns `ret`.
struct PadModel {
    calls: Vec<(u64, u32, u32)>,
    ctr: *mut i32,
    ret: u32,
}

impl Helpers for PadModel {
    unsafe fn call(&mut self, f: u64, a: u32, b: u32, _env: &Env, _frame: *mut u8) -> HelperOut {
        self.calls.push((f, a, b));
        // SAFETY: `ctr` points at a live `i32` owned by the test.
        unsafe { *self.ctr = 1_000_000 };
        HelperOut {
            result: self.ret,
            new_frame: None,
        }
    }
}

/// Region of a function with failure accounting for nest 0, code version 5.
fn region_with_pads(f: &Func, irq: *const u8, fb: FeedbackSpec) -> (Lowered, PolicyCtl) {
    let nests = scan_nests(&f.code);
    let plans: Vec<NestPlan> = nests
        .iter()
        .enumerate()
        .map(|(i, &(head, end))| NestPlan {
            nest: i as u32,
            head,
            end,
        })
        .collect();
    let meta = FuncMeta {
        max_stack: f.max_stack,
        escaped: SlotSet::EMPTY,
        obs: obs_for(0, f.max_stack as usize, plans.len()),
        prof: vec![0xFF; f.code.len()],
        intr: Vec::new(),
        opt: OptFlags::parse("+force"),
    };
    let mut ctl = PolicyCtl {
        nests: vec![NestPolicy {
            fb: Some(fb),
            ..NestPolicy::default()
        }],
        sites: Vec::new(),
    };
    let low = lower_with_regions_ex(&f.code, cfg(f, irq), &plans, Some(&meta), Some(&mut ctl))
        .expect("lowers");
    (low, ctl)
}

fn run_pads(low: &Lowered, m: &mut Mem, ret: u32, ctr: &mut i32) -> (Vec<(u64, u32, u32)>, Stop) {
    let (_, l) = *low.resume.iter().find(|(pc, _)| *pc == 0).expect("entry");
    let env = m.env();
    let mut model = PadModel {
        calls: Vec::new(),
        ctr: ctr as *mut i32,
        ret,
    };
    let mut e = Emu::new(&low.prog, l);
    // SAFETY: the buffers in `m` are large enough for every offset the templates use; the model obeys `Helpers`.
    let u = unsafe { e.run(&low.prog, &[], &env, 5_000_000, &mut model) };
    let Until::Stop(s) = u else { panic!("{u:?}") };
    (model.calls, s)
}

fn run_tier1(t1: &Lowered, m: &mut Mem) -> Stop {
    let (_, l) = *t1.resume.iter().find(|(pc, _)| *pc == 0).unwrap();
    let env = m.env();
    // SAFETY: as in `run_emu`.
    unsafe { emu::run(&t1.prog, l, &env, 5_000_000) }
}

#[test]
fn failure_pads_count_fire_the_event_and_continue_in_tier_1() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    let f = func_of(PROGS.iter().find(|(n, _)| *n == "repeat_mod").unwrap().1);
    let mut ctr = Box::new(0i32);
    let mut ok = Box::new(0u32);
    let cont = Box::new(0u64);
    let mut m0 = Mem::new();
    m0.set_k(&f.consts);
    m0.setup_ci(1, 0);
    m0.set_reg(0, LUA_VNUMINT, 10);
    for r in 1..4 {
        m0.set_reg(r, LUA_VNUMINT, (r as u64) * 5);
    }
    let fb = FeedbackSpec {
        ver: 5,
        nest: 0,
        fail_addr: (&raw mut *ctr) as usize,
        ok_addr: (&raw mut *ok) as usize,
        cont_addr: (&raw const *cont) as usize,
        event_fn: FAKE_EVENT,
    };
    let irq = Mem::new().irq;
    let (low, ctl) = region_with_pads(&f, irq.as_ptr(), fb);
    let t1 = tier1(&f, irq.as_ptr());
    assert!(low.prog.ops.len() > t1.prog.ops.len(), "no region");
    let (h, _) = scan_nests(&f.code)[0];
    // the stub of the head: one site, guarded slots (the parameters), none of them a peel slot
    let site = ctl
        .sites
        .iter()
        .find(|(n, s)| *n == 0 && s.pc == h)
        .map(|(_, s)| s.clone())
        .expect("the head stub is a site");
    assert!(!site.guards.is_empty() && site.guards.iter().all(|g| !g.peel));

    // (a) all integers: the head stub passes once (entry counter), no failure, no event
    *ctr = 3;
    let mut m = m0.clone();
    let (calls, stop) = run_pads(&low, &mut m, 0, &mut ctr);
    assert!(matches!(stop, Stop::Ret(_)));
    assert!(calls.is_empty(), "{calls:?}");
    assert_eq!((*ctr, *ok), (3, 1));
    let mut mt = m0.clone();
    assert_eq!(stop, run_tier1(&t1, &mut mt));
    assert_eq!(m.frame, mt.frame, "region result differs from tier 1");

    // (b) a float parameter: the head stub fails at every pass of the (tier-1) loop; the countdown fires once
    //     after four failures, the model re-arms it, tier 1 finishes the loop
    *ctr = 3;
    *ok = 0;
    let mut m = m0.clone();
    m.set_reg(3, LUA_VNUMFLT, 1.5f64.to_bits());
    let (calls, stop) = run_pads(&low, &mut m, 0, &mut ctr);
    let mut mt = m0.clone();
    mt.set_reg(3, LUA_VNUMFLT, 1.5f64.to_bits());
    assert_eq!(stop, run_tier1(&t1, &mut mt));
    assert_eq!(m.frame, mt.frame, "pads changed the result");
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(calls[0], (FAKE_EVENT, 5 << 24, (3 << 16) | h));
    assert!(
        *ctr < 1_000_000 && *ctr > 0,
        "re-armed, then counting: {ctr}"
    );
    assert_eq!(*ok, 0, "a failed stub is no successful entry");

    // (c) the countdown is already below zero (left by a stale version's fire without re-arming): every failure
    //     fires (32-bit compare), not only the exact 0 -> -1 step
    *ctr = -1;
    let mut m = m0.clone();
    m.set_reg(3, LUA_VNUMFLT, 1.5f64.to_bits());
    let (calls, _) = run_pads(&low, &mut m, 0, &mut ctr);
    assert_eq!(calls.len(), 1, "{calls:?}");
    // and further below zero
    *ctr = -5;
    let mut m = m0.clone();
    m.set_reg(3, LUA_VNUMFLT, 1.5f64.to_bits());
    let (calls, _) = run_pads(&low, &mut m, 0, &mut ctr);
    assert_eq!(calls.len(), 1, "{calls:?}");
}

// ---------------------------------------------------------------------- T6 review: failure pad of an inline intrinsic

/// Events (`FAKE_EVENT`) are recorded and answered with `ret` (the countdown is re-armed like the real helper does); the
/// tier-1 `CALL` helper reports an error, which ends both runs the same way.
struct PadMath {
    events: u64,
    ctr: *mut i32,
    ret: u32,
}

impl Helpers for PadMath {
    unsafe fn call(&mut self, f: u64, _a: u32, _b: u32, _env: &Env, _frame: *mut u8) -> HelperOut {
        if f == FAKE_EVENT {
            self.events += 1;
            // SAFETY: `ctr` points at a live `i32` owned by the test.
            unsafe { *self.ctr = 1_000_000 };
            return HelperOut {
                result: self.ret,
                new_frame: None,
            };
        }
        HelperOut {
            result: jh::H_ERR,
            new_frame: None,
        }
    }
}

fn region_intr_pads(f: &Func, irq: *const u8, kind: u8, fb: FeedbackSpec) -> Lowered {
    use rn_core::ver::active::OpCode as O;
    let nests = scan_nests(&f.code);
    let plans: Vec<NestPlan> = nests
        .iter()
        .enumerate()
        .map(|(i, &(head, end))| NestPlan {
            nest: i as u32,
            head,
            end,
        })
        .collect();
    let mut obs = obs_for(0, f.max_stack as usize, plans.len());
    for o in &mut obs {
        o[5] = LUA_VLCF;
    }
    let calls: Vec<u32> = f
        .code
        .iter()
        .enumerate()
        .filter(|(_, &w)| rn_core::instruction::Instruction(w).opcode() == Some(O::Call))
        .map(|(pc, _)| pc as u32)
        .collect();
    let meta = FuncMeta {
        max_stack: f.max_stack,
        escaped: SlotSet::EMPTY,
        obs,
        prof: vec![0xFF; f.code.len()],
        intr: calls
            .iter()
            .map(|&pc| {
                (
                    pc,
                    rn_jit::t15::intrinsics::Intr::from_index(kind).expect("kind"),
                )
            })
            .collect(),
        opt: OptFlags::parse("+force"),
    };
    let mut ctl = PolicyCtl {
        nests: vec![NestPolicy {
            fb: Some(fb),
            ..NestPolicy::default()
        }],
        sites: Vec::new(),
    };
    lower_with_regions_ex(
        &f.code,
        calls_cfg(f, irq),
        &plans,
        Some(&meta),
        Some(&mut ctl),
    )
    .expect("lowers")
}

/// Only the failures a demotion can cure (callee identity, stack room, spare `CallInfo`) feed the countdown; a hook does
/// not. When the policy helper stays (returns 0) the run must be indistinguishable from tier 1's: frame, budget,
/// `CallInfo`, thread (`r11` is clobbered by the helper and reloaded from `[r15]`).
#[test]
fn intrinsic_failure_pad_counts_demotable_failures_only_and_stays_exactly() {
    if rn_core::jit_layout::LUA54 {
        return;
    }
    register_intrinsics();
    let src = IN_PROGS_EMU
        .iter()
        .find(|(n, _, _)| *n == "in_sqrt_int")
        .unwrap();
    let f = func_of(src.2);
    let addr = rn_core::jit_hooks::intrinsic_addr(src.1) as u64;
    for scn in [
        Scn::Hook,
        Scn::StackFull,
        Scn::NoSpareCi,
        Scn::WrongPayload,
        Scn::WrongTag,
    ] {
        for budget in [i32::MAX, 40, 7] {
            let mut ctr = Box::new(0i32); // fires at the first counted failure
            let mut ok = Box::new(0u32);
            let cont = Box::new(0u64);
            let fb = FeedbackSpec {
                ver: 5,
                nest: 0,
                fail_addr: (&raw mut *ctr) as usize,
                ok_addr: (&raw mut *ok) as usize,
                cont_addr: (&raw const *cont) as usize,
                event_fn: FAKE_EVENT,
            };
            let mut m = Mem::new();
            m.set_k(&f.consts);
            m.setup_ci(1 | ci::CIST_FRESH, 0);
            Mem::put(&mut m.ci, ci::PREVIOUS, dummy_ci());
            Mem::put(&mut m.ci, ci::NEXT, 0x7100usize);
            Mem::put(
                &mut m.thread,
                rn_core::jit_hooks::thread::STACK_SIZE,
                100_000usize,
            );
            m.set_reg(0, LUA_VNUMINT, 6);
            m.set_reg(1, LUA_VNUMINT, 3);
            for r in 2..5 {
                m.set_reg(r, LUA_VNUMINT, 1);
            }
            m.set_reg(5, LUA_VLCF, addr);
            m.cnt = budget;
            match scn {
                Scn::Normal => {}
                Scn::Hook => Mem::put(&mut m.thread, rn_core::jit_hooks::thread::HOOKMASK, 1u8),
                Scn::StackFull => Mem::put(
                    &mut m.thread,
                    rn_core::jit_hooks::thread::STACK_SIZE,
                    30usize,
                ),
                Scn::NoSpareCi => Mem::put(&mut m.ci, ci::NEXT, 0usize),
                Scn::WrongPayload => m.set_reg(5, LUA_VLCF, addr.wrapping_add(8)),
                Scn::WrongTag => m.set_reg(5, rn_core::value::LUA_VLCFC, addr),
                Scn::Irq => {}
            }
            let mut m1 = m.clone();
            let mut m2 = m.clone();
            let plain = tier1_calls(&f, m1.irq.as_ptr());
            let low = region_intr_pads(&f, m2.irq.as_ptr(), src.1, fb);
            let mut ctr1 = 0i32;
            let (_, l1) = *plain.resume.iter().find(|(pc, _)| *pc == 0).unwrap();
            let env1 = m1.env();
            let mut mod1 = PadMath {
                events: 0,
                ctr: &mut ctr1,
                ret: 0,
            };
            let mut e1 = Emu::new(&plain.prog, l1);
            // SAFETY: the buffers in `m1` are large enough for every offset the templates use.
            let u1 = unsafe { e1.run(&plain.prog, &[], &env1, 5_000_000, &mut mod1) };
            let (_, l2) = *low.resume.iter().find(|(pc, _)| *pc == 0).unwrap();
            let env2 = m2.env();
            let mut mod2 = PadMath {
                events: 0,
                ctr: &raw mut *ctr,
                ret: 0,
            };
            let mut e2 = Emu::new(&low.prog, l2);
            // SAFETY: as above, for `m2`.
            let u2 = unsafe { e2.run(&low.prog, &[], &env2, 5_000_000, &mut mod2) };
            let what = format!("{scn:?} budget {budget}");
            assert_eq!(u1, u2, "{what}: end");
            assert_eq!(m1.frame, m2.frame, "{what}: frame");
            assert_eq!(m1.cnt, m2.cnt, "{what}: budget");
            assert_eq!(m1.ci, m2.ci, "{what}: ci");
            assert_eq!(m1.thread, m2.thread, "{what}: thread");
            assert_eq!(mod1.events, 0);
            let region_ran = !low.points.is_empty();
            if region_ran && budget == i32::MAX {
                match scn {
                    Scn::Hook | Scn::Normal | Scn::Irq => {
                        assert_eq!(
                            mod2.events, 0,
                            "{what}: a non-demotable slow path was counted"
                        )
                    }
                    _ => assert!(mod2.events >= 1, "{what}: the failure was not counted"),
                }
            }
        }
    }
}
