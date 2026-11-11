//! J1b: every arithmetic / bitwise / comparison / test template against the interpreter, on special value
//! patterns, and the x86-64 backend against the emulator.
//!
//! A tiny Lua function `function(a, b) <body> end` is compiled by the real compiler; the interpreter (this
//! test binary never registers the JIT) gives the reference result for every pair of values. The same
//! bytecode is lowered to micro-ops and run by the Rust emulator on a synthetic frame (and, on x86-64, by the
//! machine code). A template may *exit* (it then must not have modified the arguments and must stop inside the
//! function) but when it completes, its result must be the interpreter's.
#![cfg(feature = "jit")]

use rn_core::jit_layout::callinfo as ci;
use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_core::value::{
    Value, LUA_VFALSE, LUA_VNIL, LUA_VNUMFLT, LUA_VNUMINT, LUA_VSHRSTR, LUA_VTRUE,
};
use rn_jit::emu::{self, Env, Stop};
use rn_jit::lower::{lower, LowerCfg, Lowered};

const FAKE_CODE: usize = 0x10_0000;
const PRE: usize = 2;
const NSLOT: usize = 24;

#[derive(Clone, Copy, Debug)]
enum V {
    I(i64),
    F(f64),
    Nil,
    T,
    Fa,
    S(&'static str),
}

impl V {
    fn raw(self) -> (u8, u64) {
        match self {
            V::I(x) => (LUA_VNUMINT, x as u64),
            V::F(f) => (LUA_VNUMFLT, f.to_bits()),
            V::Nil => (LUA_VNIL, 0),
            V::T => (LUA_VTRUE, 0),
            V::Fa => (LUA_VFALSE, 0),
            V::S(_) => (LUA_VSHRSTR | 0x40, 0x1000),
        }
    }
    fn is_int(self) -> bool {
        matches!(self, V::I(_))
    }
    fn is_flt(self) -> bool {
        matches!(self, V::F(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Out {
    Int(i64),
    Flt(u64),
    Bool(bool),
    Nil,
    Other(u8),
    Err,
}

fn canon(f: f64) -> u64 {
    if f.is_nan() {
        f64::NAN.to_bits()
    } else {
        f.to_bits()
    }
}

fn out_of(tag: u8, payload: u64) -> Out {
    match tag {
        LUA_VNUMINT => Out::Int(payload as i64),
        LUA_VNUMFLT => Out::Flt(canon(f64::from_bits(payload))),
        LUA_VTRUE => Out::Bool(true),
        LUA_VFALSE => Out::Bool(false),
        LUA_VNIL => Out::Nil,
        t => Out::Other(t & 0x3F),
    }
}

fn values() -> Vec<V> {
    let mut v = vec![
        V::I(0),
        V::I(1),
        V::I(-1),
        V::I(2),
        V::I(-2),
        V::I(3),
        V::I(5),
        V::I(7),
        V::I(63),
        V::I(64),
        V::I(65),
        V::I(-63),
        V::I(-64),
        V::I(1 << 53),
        V::I((1 << 53) + 1),
        V::I(-(1 << 53) - 1),
        V::I(1 << 62),
        V::I(i64::MAX),
        V::I(i64::MIN),
        V::I(i64::MIN + 1),
        V::F(0.0),
        V::F(-0.0),
        V::F(1.0),
        V::F(-1.0),
        V::F(2.5),
        V::F(5.0),
        V::F(0.5),
        V::F(1e300),
        V::F(-1e300),
        V::F(f64::NAN),
        V::F(f64::INFINITY),
        V::F(f64::NEG_INFINITY),
        V::F(9.223_372_036_854_776e18),
        V::F(-9.223_372_036_854_776e18),
        V::F(9007199254740992.0),
        V::F(1e-300),
        V::Nil,
        V::T,
        V::Fa,
    ];
    for s in ["10", "0x10", "abc", "1e2", " 5 ", ""] {
        v.push(V::S(s));
    }
    v
}

/// All memory a program touches (as in `emu_diff.rs`).
#[derive(Clone)]
struct Mem {
    frame: Vec<u64>,
    ci: Vec<u64>,
    thread: Vec<u64>,
    k: Vec<u64>,
    cnt: i32,
}

impl Mem {
    fn new(kbytes: &[u8]) -> Mem {
        let mut m = Mem {
            frame: vec![0; (NSLOT + PRE) * 2],
            ci: vec![0; ci::SIZE.div_ceil(8)],
            thread: vec![0; 64],
            k: vec![0; kbytes.len().div_ceil(8) + 2],
            cnt: i32::MAX,
        };
        // SAFETY: the constants buffer is at least `kbytes.len()` bytes.
        unsafe {
            std::ptr::copy_nonoverlapping(
                kbytes.as_ptr(),
                m.k.as_mut_ptr().cast::<u8>(),
                kbytes.len(),
            );
        }
        m
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
                .write_unaligned(v)
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
    fn setup_ci(&mut self) {
        Self::put(&mut self.ci, ci::CALLSTATUS, 2u32); // one result wanted
        Self::put(&mut self.ci, ci::TRAP, 0i32);
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
    fn result(&self) -> Out {
        out_of(
            self.frame[(PRE - 1) * 2] as u8,
            self.frame[(PRE - 1) * 2 + 1],
        )
    }
    fn args(&self) -> [u64; 4] {
        [
            self.frame[PRE * 2],
            self.frame[PRE * 2 + 1],
            self.frame[(PRE + 1) * 2],
            self.frame[(PRE + 1) * 2 + 1],
        ]
    }
}

/// A compiled test function: bytecode, constants and the interpreter closure.
struct Case {
    body: String,
    st: LuaState,
    func_idx: rn_core::state::StkId,
    low: Lowered,
    kbytes: Vec<u8>,
    /// The test strings as real (interned, anchored on the stack) objects: the emulated registers must hold
    /// the same pointers the interpreter would see (`EQK` of a short string compares pointers).
    strs: Vec<(&'static str, (u8, u64))>,
    #[cfg(not(miri))]
    native: Native,
    ncode: usize,
}

#[cfg(not(miri))]
struct Native {
    thunk: rn_jit::x86::Thunk,
    entry: usize,
    _arena: rn_jit::mem::CodeArena,
}

fn build(body: &str) -> Case {
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    let src = format!("return function(a, b) {body} end");
    assert_eq!(
        st.load_buffer(src.as_bytes(), b"=t", None),
        Status::Ok,
        "{body}"
    );
    let f = st.top() - 1;
    assert_eq!(st.pcall(f, 1, 0), Status::Ok, "{body}");
    let func_idx = st.top() - 1; // the test function stays here
    let mut strs = Vec::new();
    for v in values() {
        if let V::S(name) = v {
            let val = st.new_str_value(name.as_bytes()).expect("string");
            st.push(val);
            strs.push((
                name,
                (val.tag(), val.as_gc().expect("string") as usize as u64),
            ));
        }
    }
    let cl = st.get(func_idx).as_lclosure().expect("closure");
    let p = cl.proto();
    let code: Vec<u32> = p.code().to_vec();
    let consts: Vec<Value> = p.k().to_vec();
    let irq: &'static u8 = Box::leak(Box::new(0u8));
    let low = lower(
        &code,
        LowerCfg {
            code_addr: FAKE_CODE,
            interrupt_addr: std::ptr::from_ref(irq) as usize,
            consts: &consts,
            t15_warmup: None,
            calls: None,
        },
    )
    .expect("lowers");
    // SAFETY: `Value` is 16 bytes of plain data; the slice covers exactly the constants.
    let kbytes = unsafe {
        std::slice::from_raw_parts(consts.as_ptr().cast::<u8>(), consts.len() * 16).to_vec()
    };
    #[cfg(not(miri))]
    let native = {
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
        // SAFETY: `tb` holds the committed thunk; `Thunk` is its signature.
        let thunk: Thunk = unsafe { arena.entry(&tb) }.unwrap();
        Native {
            thunk,
            entry: cb.addr() + off,
            _arena: arena,
        }
    };
    Case {
        body: body.to_string(),
        st,
        func_idx,
        low,
        kbytes,
        strs,
        #[cfg(not(miri))]
        native,
        ncode: code.len(),
    }
}

impl Case {
    fn interp(&mut self, a: V, b: V) -> Out {
        let base = self.st.top();
        let fval = self.st.get(self.func_idx);
        self.st.push(fval);
        for v in [a, b] {
            let val = match v {
                V::I(x) => Value::int(x),
                V::F(f) => Value::float(f),
                V::Nil => Value::NIL,
                V::T => Value::TRUE,
                V::Fa => Value::FALSE,
                V::S(s) => self.st.new_str_value(s.as_bytes()).expect("string"),
            };
            self.st.push(val);
        }
        let f = base;
        let status = self.st.pcall(f, 1, 0);
        let out = if status == Status::Ok {
            let r = self.st.get(f);
            match r.tag() {
                LUA_VNUMINT => Out::Int(r.as_int().unwrap()),
                LUA_VNUMFLT => Out::Flt(canon(r.as_float().unwrap())),
                LUA_VTRUE => Out::Bool(true),
                LUA_VFALSE => Out::Bool(false),
                LUA_VNIL => Out::Nil,
                t => Out::Other(t & 0x3F),
            }
        } else {
            Out::Err
        };
        self.st.set_top(base);
        let _ = LUA_MULTRET;
        out
    }

    fn start_mem(&self, a: V, b: V) -> Mem {
        let mut m = Mem::new(&self.kbytes);
        m.setup_ci();
        let raw = |v: V| match v {
            V::S(n) => {
                self.strs
                    .iter()
                    .find(|(s, _)| *s == n)
                    .expect("known string")
                    .1
            }
            v => v.raw(),
        };
        let (ta, pa) = raw(a);
        let (tb, pb) = raw(b);
        m.set_reg(0, ta, pa);
        m.set_reg(1, tb, pb);
        m
    }

    fn emulate(&self, m: &mut Mem) -> u32 {
        let (_, l) = *self.low.resume.iter().find(|(pc, _)| *pc == 0).unwrap();
        let env = m.env();
        // SAFETY: the buffers in `m` are large enough for every offset the templates use.
        match unsafe { emu::run(&self.low.prog, l, &env, 1_000_000) } {
            Stop::Ret(c) => c,
            Stop::OutOfSteps => panic!("emulator did not finish: {}", self.body),
            Stop::Unsupported => panic!("native-only op in a template: {}", self.body),
        }
    }

    #[cfg(not(miri))]
    fn run_native(&self, m: &mut Mem) -> u32 {
        let env = m.env();
        // SAFETY: as above; `entry` is the committed resume address of pc 0 of this case's code.
        unsafe {
            (self.native.thunk)(
                env.thread,
                env.ci,
                env.frame,
                env.cnt,
                self.native.entry as *const u8,
            )
        }
    }
}

struct Tally {
    completed: usize,
    exited: usize,
}

/// Checks one body over `pairs`. `ints_complete` / `floats_complete`: pairs of two integers / two floats
/// (excluding NaN-free requirements) must not exit.
fn check(body: &str, pairs: &[(V, V)], ints_complete: bool, floats_complete: bool) -> Tally {
    let mut c = build(body);
    let mut t = Tally {
        completed: 0,
        exited: 0,
    };
    for &(a, b) in pairs {
        let want = c.interp(a, b);
        let mut me = c.start_mem(a, b);
        let before = me.args();
        let code_e = c.emulate(&mut me);
        let ctx = || format!("`{body}` with a = {a:?}, b = {b:?}");
        match code_e {
            1 | 2 => {
                t.completed += 1;
                assert_eq!(me.result(), want, "result differs: {}", ctx());
            }
            0 => {
                t.exited += 1;
                let pc: usize = Mem::get(&me.ci, ci::SAVEDPC);
                assert!(
                    (FAKE_CODE..FAKE_CODE + 4 * c.ncode).contains(&pc),
                    "savedpc outside the function: {}",
                    ctx()
                );
                assert_eq!(
                    me.args(),
                    before,
                    "arguments modified before an exit: {}",
                    ctx()
                );
                if ints_complete && a.is_int() && b.is_int() {
                    panic!("int/int pair exited: {}", ctx());
                }
                if floats_complete && a.is_flt() && b.is_flt() {
                    panic!("float/float pair exited: {}", ctx());
                }
            }
            x => panic!("exit code {x}: {}", ctx()),
        }
        #[cfg(not(miri))]
        {
            let mut mn = c.start_mem(a, b);
            let code_n = c.run_native(&mut mn);
            assert_eq!(code_e, code_n, "native exit code: {}", ctx());
            assert_eq!(me.frame, mn.frame, "native frame: {}", ctx());
            assert_eq!(me.thread, mn.thread, "native thread: {}", ctx());
            assert_eq!(me.cnt, mn.cnt, "native budget: {}", ctx());
            assert_eq!(
                Mem::get::<usize>(&me.ci, ci::SAVEDPC),
                Mem::get::<usize>(&mn.ci, ci::SAVEDPC),
                "native savedpc: {}",
                ctx()
            );
        }
    }
    t
}

fn all_pairs() -> Vec<(V, V)> {
    let vs = values();
    let mut out = Vec::new();
    for &a in &vs {
        for &b in &vs {
            out.push((a, b));
        }
    }
    out
}

/// The first operand with a fixed second operand of a few kinds (for K / immediate forms).
fn a_pairs() -> Vec<(V, V)> {
    values().into_iter().map(|a| (a, V::I(0))).collect()
}

fn run_group(bodies: &[&str], pairs: &[(V, V)], ints: bool, floats: bool) {
    for b in bodies {
        let t = check(b, pairs, ints, floats);
        eprintln!("{b}: {} completed, {} exited", t.completed, t.exited);
        assert!(
            t.completed > 0,
            "`{b}` never completes the fast path ({} exits)",
            t.exited
        );
    }
}

#[test]
fn add_sub_mul() {
    let bodies = ["return a + b", "return a - b", "return a * b"];
    run_group(&bodies, &all_pairs(), true, true);
}

#[test]
fn arith_with_constants() {
    let bodies = [
        "return a + 1",
        "return a + 100",
        "return a - 3",
        "return a + 1.5",
        "return a - 2.5",
        "return a * 2.5",
        "return a * 7",
        "return a * 1000000",
        "return a + 4294967296",
        "return a * 4294967296",
        "return a + 0.0",
        "return a - 1e300",
    ];
    run_group(&bodies, &a_pairs(), true, true);
}

#[test]
fn division_family() {
    run_group(&["return a / b"], &all_pairs(), true, true);
    // `//` and `%`: integers (including zero and -1 divisors) complete or raise (the interpreter errors, the
    // template must have exited); floats exit
    for body in ["return a // b", "return a % b"] {
        let t = check(body, &all_pairs(), false, false);
        assert!(t.completed > 100, "`{body}`: {} completed", t.completed);
    }
    run_group(
        &[
            "return a / 2",
            "return a / 0.5",
            "return a // 3",
            "return a % 3",
            "return a // -1",
            "return a % -1",
            "return a // -3",
            "return a % -7",
        ],
        &a_pairs(),
        false,
        false,
    );
    // float divisors and zero divisors always take the interpreter
    for body in [
        "return a // 2.5",
        "return a % 2.5",
        "return a // 0",
        "return a % 0",
    ] {
        let t = check(body, &a_pairs(), false, false);
        assert_eq!(t.completed, 0, "`{body}`");
    }
}

#[test]
fn bitwise_and_shifts() {
    let bodies = [
        "return a & b",
        "return a | b",
        "return a ~ b",
        "return a << b",
        "return a >> b",
    ];
    run_group(&bodies, &all_pairs(), true, false);
    let bodies = [
        "return a & 0xFF",
        "return a | 3",
        "return a ~ 5",
        "return a ~ -1",
        "return a << 3",
        "return a >> 2",
        "return a >> -2",
        "return a << 70",
        "return a >> 64",
        "return a << 63",
        "return a >> 0",
        "return 3 << a",
        "return -1 << a",
        "return ~a",
        "return -a",
        "return a & 0x7FFFFFFFFFFFFFFF",
        "return a | 0x100000000",
    ];
    run_group(&bodies, &a_pairs(), true, false);
}

#[test]
fn comparisons() {
    let bodies = [
        "return a < b",
        "return a <= b",
        "return a > b",
        "return a >= b",
        "return a == b",
        "return a ~= b",
    ];
    for b in bodies {
        check(b, &all_pairs(), true, true);
    }
    let bodies = [
        "return a < 5",
        "return a <= 5",
        "return a > 5",
        "return a >= 5",
        "return a < -3",
        "return a > 100",
        "return a == 5",
        "return a ~= 5",
        "return a == -1",
        "return a == 2.5",
        "return a ~= 2.5",
        "return a < 2.5",
        "return a >= 1e100",
        "return a == 9007199254740993",
        "return a == 9007199254740992.0",
        "return a == nil",
        "return a ~= nil",
        "return a == true",
        "return a == false",
        "return a == 'abc'",
    ];
    for b in bodies {
        check(b, &a_pairs(), false, false);
    }
}

#[test]
fn tests_and_logic() {
    let bodies = [
        "return a and b",
        "return a or b",
        "if a then return 1 else return 2 end",
        "if not a then return 1 else return 2 end",
        "return not a",
        "local c = a if c then return b end return c",
        "return a and b or 7",
        "if a == b then return 1 end return 2",
        "if a < b then return 1 end return 2",
        "if a <= b and b < a then return 1 end return 2",
    ];
    run_group(&bodies, &all_pairs(), false, false);
}

/// Mixed expression bodies exercise the type knowledge inside a block (second operation sees a known type).
#[test]
fn block_type_knowledge() {
    let bodies = [
        "local c = a + 1 return c * c - a",
        "local c = a * 2.0 local d = c + c return d - c",
        "local c = a // 3 return c % 5 + c",
        "local c = a & 255 local d = c << 4 return d | c",
        "local c = 3 local d = c + a return d < 10",
        "local c = 1.5 return a < c",
        "local c = a + 0.5 return c == a",
        "local c = -a return c * c",
        "local c = a c = 2 return c * a",
    ];
    run_group(&bodies, &all_pairs(), false, false);
}
