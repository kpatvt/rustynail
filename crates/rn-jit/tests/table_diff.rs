//! J1d: the table templates (GETI GETTABLE GETFIELD GETTABUP SELF SETI SETTABLE SETFIELD SETTABUP) against the
//! interpreter on *real* `Table` objects (arrays with holes, collisions, deleted and dead keys, metatables,
//! integer-valued float keys, zero / negative / huge indexes), by the Rust emulator and by the machine code.
//!
//! A tiny Lua function `function(a, b, c) <body> end` (with an upvalue `T` for the GETTABUP / SETTABUP forms) is
//! compiled by the real compiler. Four identical tables are built per run: a pristine one, one for the
//! interpreter, one for the emulator and one for the native code. A template may exit (then the tables and the
//! arguments must be untouched) but when it completes, the result and the final table contents must be the
//! interpreter's, and it must not have skipped a GC barrier (checked in `barrier_matrix`).
#![cfg(feature = "jit")]

use rn_core::gc::{BLACKBIT, WHITEBITS};
use rn_core::instruction::Instruction;
use rn_core::jit_layout::{callinfo as ci, table};
use rn_core::state::{LuaState, StkId};
use rn_core::status::Status;
use rn_core::value::Value;
use rn_core::ver::active::OpCode;
use rn_jit::emu::{self, Env, Stop};
use rn_jit::lower::{lower, LowerCfg, Lowered};

const FAKE_CODE: usize = 0x10_0000;
const PRE: usize = 2;
const NSLOT: usize = 24;

const PRELUDE: &str = r#"
R = {}
local function keep(x) R[#R + 1] = x; return x end
function canon(v)
  local ty = math.type(v)
  if ty == "integer" then return "i" .. v
  elseif ty == "float" then return string.format("f%a", v)
  elseif type(v) == "string" then return "s" .. v
  elseif v == nil then return "nil"
  elseif type(v) == "boolean" then return "b" .. tostring(v)
  else return type(v) end
end
function dump(t, withlen)
  local out = {}
  local k, v = next(t)
  while k ~= nil do
    out[#out + 1] = canon(k) .. "=" .. canon(v)
    k, v = next(t, k)
  end
  table.sort(out)
  -- '#t' observes the array limit hint (5.4 alimit, updated by every integer access): with 'withlen'
  -- the border is part of the dump (not for the "unchanged before an exit" check: an exit may follow the touch)
  return table.concat(out, ";") .. "|" .. tostring(getmetatable(t) ~= nil) .. (withlen and ("|#" .. #t) or "")
end
function reset() R = {} end
function mk(n)
  local t
  if n == 1 then t = {}
  elseif n == 2 then t = {10, 20, 30}
  elseif n == 3 then t = {10, nil, 30, nil, 50}
  elseif n == 4 then t = {1, 2, 3, 4, x = 1, y = 2.5, z = "str", name = true}
  elseif n == 5 then
    t = {}
    for i = 1, 24 do t["k" .. i] = i end
    t.k5 = nil; t.k6 = nil; t.k13 = nil
  elseif n == 6 then
    t = setmetatable({1, 2, 3, nil, x = 5, y = 7},
      {__index = function(_, k) return "mm" end, __newindex = function(t, k, v) rawset(t, 'ni', canon(k)) end})
  elseif n == 7 then
    t = {x = 1, k1 = 2}
    for i = 1, 6 do t[{}] = i end
    for k in pairs(t) do if type(k) == "table" then t[k] = nil end end
    collectgarbage()
  elseif n == 8 then t = setmetatable({5, 6, 7, x = 1}, {})
  elseif n == 9 then
    t = {}
    t[1] = 1; t[2] = 2; t[100] = 3; t[-1] = 4; t[0] = 5; t[1 << 40] = 6; t[2.5] = 7; t.x = 8
  elseif n == 10 then t = table.pack(1, 2, 3)
  elseif n == 11 then t = setmetatable({1, 2, nil, x = 1}, {__index = function() return "idx" end})
  elseif n == 12 then t = {[1] = 1, [2] = 2, [3] = 3, x = 1, k24 = 24}
  elseif n == 13 then t = {1, 2, 3, x = 1, k1 = 1, k2 = 2, k3 = 3, k4 = 4}; t.k1 = nil; t.x = nil
  elseif n == 14 then t = {}; for i = 1, 300 do t[i] = i end; t.x = 1
  elseif n == 15 then
    -- 5.4: '#t' leaves the array limit below the real size (BITRAS), accesses at or above it move it
    t = {1, 2, 3, 4, 5, 6, 7, 8}; t[8] = nil; t[7] = nil; local _ = #t
  elseif n == 16 then
    t = {}; for i = 1, 20 do t[i] = i end
    t[20] = nil; t[19] = nil; t[18] = nil; t[9] = nil; t[3] = nil; local _ = #t; local _ = #t
  elseif n == 18 then
    -- non-nil elements above the limit hint (limit 7 of a 16-slot array)
    t = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16}; t[16] = nil; t[15] = nil; t[8] = nil; local _ = #t
  elseif n == 17 then
    t = {10, 20, 30, 40, 50, 60, 70, 80, 90, 100}; t[10] = nil; t[9] = nil; t[8] = nil; local _ = #t
  end
  return keep(t)
end
NTABS = 13
function arg(i)
  if i == 1 then return 1 elseif i == 2 then return 2 elseif i == 3 then return 3 elseif i == 4 then return 4
  elseif i == 5 then return 5 elseif i == 6 then return 0 elseif i == 7 then return -1
  elseif i == 8 then return 100 elseif i == 9 then return 1 << 40 elseif i == 10 then return math.maxinteger
  elseif i == 11 then return math.mininteger elseif i == 12 then return 2.0 elseif i == 13 then return 3.0
  elseif i == 14 then return 2.5 elseif i == 15 then return 1e100 elseif i == 16 then return "x"
  elseif i == 17 then return "y" elseif i == 18 then return "name" elseif i == 19 then return "k1"
  elseif i == 20 then return "k5" elseif i == 21 then return "k13" elseif i == 22 then return "k24"
  elseif i == 23 then return "absent" elseif i == 24 then return true elseif i == 25 then return nil
  elseif i == 26 then return 0/0 elseif i == 27 then return -0.0 elseif i == 28 then return keep({})
  elseif i == 29 then return "z" elseif i == 30 then return -(1 << 40)
  elseif i == 31 then return keep({}) elseif i == 32 then return keep("fresh" .. #R)
  elseif i == 33 then return keep(function() end) elseif i == 34 then return 4294967296
  elseif i == 35 then return 2147483648
  elseif i == 36 then return 6 elseif i == 37 then return 7 elseif i == 38 then return 8
  elseif i == 39 then return 9 elseif i == 40 then return 10 elseif i == 41 then return 18
  elseif i == 42 then return 20 elseif i == 43 then return 21
  elseif i == 44 then return 11 elseif i == 45 then return 14 elseif i == 46 then return 12 end
end
"#;

const NARGS: i64 = 46;

fn raw(v: Value) -> [u64; 2] {
    // SAFETY: `Value` is 16 bytes of `repr(C)` data (tag word, payload).
    unsafe { std::mem::transmute::<Value, [u64; 2]>(v) }
}
fn unraw(r: [u64; 2]) -> Value {
    // SAFETY: as above; only values that came from `raw` or a template copy are rebuilt.
    unsafe { std::mem::transmute::<[u64; 2], Value>(r) }
}

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
    fn put<T: Copy>(buf: &mut [u64], off: usize, v: T) {
        assert!(off + std::mem::size_of::<T>() <= buf.len() * 8);
        // SAFETY: bounds checked above.
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
    fn setup(&mut self, closure: Value, regs: &[Value]) {
        Self::put(&mut self.ci, ci::CALLSTATUS, 2u32);
        Self::put(&mut self.ci, ci::TRAP, 0i32);
        Self::put(&mut self.ci, ci::PREVIOUS, 0x7000usize);
        Self::put(&mut self.ci, ci::FUNC, 5usize);
        Self::put(&mut self.ci, ci::KCACHE, self.k.as_ptr() as usize);
        let c = raw(closure);
        self.frame[(PRE - 1) * 2] = c[0];
        self.frame[(PRE - 1) * 2 + 1] = c[1];
        for (r, v) in regs.iter().enumerate() {
            let x = raw(*v);
            self.frame[(PRE + r) * 2] = x[0];
            self.frame[(PRE + r) * 2 + 1] = x[1];
        }
    }
    fn reg(&self, r: usize) -> [u64; 2] {
        [self.frame[(PRE + r) * 2], self.frame[(PRE + r) * 2 + 1]]
    }
    fn result(&self) -> Value {
        unraw([self.frame[(PRE - 1) * 2], self.frame[(PRE - 1) * 2 + 1]])
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

#[cfg(not(miri))]
struct Native {
    thunk: rn_jit::x86::Thunk,
    entry: usize,
    _arena: rn_jit::mem::CodeArena,
}

struct Case {
    body: String,
    st: LuaState,
    funcs: [StkId; 6], // canon dump mk arg reset factory
    low: Option<Lowered>,
    kbytes: Vec<u8>,
    #[cfg(not(miri))]
    native: Option<Native>,
    ncode: usize,
    code: Vec<u32>,
}

fn build(body: &str, gen: bool) -> Case {
    let mut st: LuaState = rn_std::new_state_with_libs().expect("state");
    if gen {
        let pre = format!("collectgarbage('generational') collectgarbage('stop') {PRELUDE} return canon, dump, mk, arg, reset, function(T) return function(a, b, c) {body} end end");
        assert_eq!(st.load_buffer(pre.as_bytes(), b"=pre", None), Status::Ok);
    } else {
        let pre = format!("collectgarbage('incremental') collectgarbage('stop') {PRELUDE} return canon, dump, mk, arg, reset, function(T) return function(a, b, c) {body} end end");
        assert_eq!(st.load_buffer(pre.as_bytes(), b"=pre", None), Status::Ok);
    }
    let f = st.top() - 1;
    assert_eq!(st.pcall(f, 6, 0), Status::Ok, "{body}");
    let first = st.top() - 6;
    let funcs = [first, first + 1, first + 2, first + 3, first + 4, first + 5];
    let mut c = Case {
        body: body.to_string(),
        st,
        funcs,
        low: None,
        kbytes: vec![],
        #[cfg(not(miri))]
        native: None,
        ncode: 0,
        code: vec![],
    };
    // get the proto from one closure
    let cl = c.call(5, &[Value::NIL]);
    let lc = cl.as_lclosure().expect("closure");
    let p = lc.proto();
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
    // SAFETY: `Value` is 16 bytes of plain data.
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
        // SAFETY: `tb` holds the committed thunk.
        let thunk: Thunk = unsafe { arena.entry(&tb) }.unwrap();
        Native {
            thunk,
            entry: cb.addr() + off,
            _arena: arena,
        }
    };
    c.low = Some(low);
    c.kbytes = kbytes;
    #[cfg(not(miri))]
    {
        c.native = Some(native);
    }
    c.ncode = code.len();
    c.code = code;
    c
}

impl Case {
    /// Calls function `fidx` (index into `funcs`) and returns its single result.
    fn call(&mut self, fidx: usize, args: &[Value]) -> Value {
        let f = self.st.get(self.funcs[fidx]);
        self.call_val(f, args)
    }
    fn call_val(&mut self, f: Value, args: &[Value]) -> Value {
        let base = self.st.top();
        self.st.check_stack(args.len() + 4).expect("stack");
        self.st.push(f);
        for a in args {
            self.st.push(*a);
        }
        let status = self.st.pcall(base, 1, 0);
        assert_eq!(status, Status::Ok, "helper call failed: {}", self.body);
        let r = self.st.get(base);
        self.st.set_top(base);
        r
    }
    fn text(&mut self, f: usize, v: Value) -> String {
        let r = self.call(f, &[v]);
        String::from_utf8_lossy(r.as_string().expect("string").as_bytes()).into_owned()
    }
    fn canon(&mut self, v: Value) -> String {
        self.text(0, v)
    }
    fn dump(&mut self, v: Value) -> String {
        self.text(1, v)
    }
    /// 'dump' plus the border (observes the array limit hint, see the prelude).
    fn dump_len(&mut self, v: Value) -> String {
        let f = self.st.get(self.funcs[1]);
        let r = self.call_val(f, &[v, Value::TRUE]);
        String::from_utf8_lossy(r.as_string().expect("string").as_bytes()).into_owned()
    }
    fn arg(&mut self, i: i64) -> Value {
        self.call(3, &[Value::int(i)])
    }
    fn mk(&mut self, n: i64) -> Value {
        self.call(2, &[Value::int(n)])
    }

    /// Runs the closure in the interpreter.
    fn interp(&mut self, clo: Value, args: [Value; 3]) -> String {
        let base = self.st.top();
        self.st.check_stack(8).expect("stack");
        self.st.push(clo);
        for v in args {
            self.st.push(v);
        }
        let status = self.st.pcall(base, 1, 0);
        if status == Status::Ok {
            let r = self.st.get(base);
            self.st.set_top(base);
            self.canon(r)
        } else {
            self.st.set_top(base);
            "ERR".to_string()
        }
    }

    fn emulate(&self, m: &mut Mem) -> u32 {
        let low = self.low.as_ref().unwrap();
        let (_, l) = *low.resume.iter().find(|(pc, _)| *pc == 0).unwrap();
        let env = m.env();
        // SAFETY: the buffers in `m` are large enough; table pointers are live tables.
        match unsafe { emu::run(&low.prog, l, &env, 1_000_000) } {
            Stop::Ret(c) => c,
            Stop::OutOfSteps => panic!("emulator did not finish: {}", self.body),
            Stop::Unsupported => panic!("native-only op in a template: {}", self.body),
        }
    }

    #[cfg(not(miri))]
    fn run_native(&self, m: &mut Mem) -> u32 {
        let env = m.env();
        let nat = self.native.as_ref().unwrap();
        // SAFETY: as above; `entry` is the resume address of pc 0.
        unsafe {
            (nat.thunk)(
                env.thread,
                env.ci,
                env.frame,
                env.cnt,
                nat.entry as *const u8,
            )
        }
    }

    fn closure_of(&mut self, tbl: Value) -> Value {
        self.call(5, &[tbl])
    }
}

#[derive(Default, Debug)]
struct Tally {
    completed: usize,
    exited: usize,
}

/// Runs one (table, b, c) combination through interpreter, emulator and native code.
fn one(c: &mut Case, n: i64, bi: i64, ci_: i64, t: &mut Tally) {
    let p_tab = c.mk(n);
    let i_tab = c.mk(n);
    let e_tab = c.mk(n);
    let n_tab = c.mk(n);
    let b = c.arg(bi);
    let cv = c.arg(ci_);
    let pristine = c.dump(p_tab);
    let fi = c.closure_of(i_tab);
    let fe = c.closure_of(e_tab);
    let fnn = c.closure_of(n_tab);
    let want = c.interp(fi, [i_tab, b, cv]);
    let after_i = c.dump_len(i_tab);
    let (bs, cs) = (c.canon(b), c.canon(cv));
    let ctx = format!(
        "`{}` with table #{n} [{pristine}], b = {bs}, c = {cs}",
        c.body
    );
    // emulator
    let mut me = Mem::new(&c.kbytes);
    me.setup(fe, &[e_tab, b, cv]);
    let before = [me.reg(0), me.reg(1), me.reg(2)];
    let code_e = c.emulate(&mut me);
    let check_state = |c: &mut Case,
                       me: &Mem,
                       before: [[u64; 2]; 3],
                       tab: Value,
                       code: u32,
                       t: &mut Tally,
                       who: &str| {
        match code {
            1 | 2 => {
                t.completed += 1;
                let r = me.result();
                let got = c.canon(r);
                assert_eq!(got, want, "{who}: result differs: {ctx}");
                let d = c.dump_len(tab);
                assert_eq!(
                    d, after_i,
                    "{who}: table differs from the interpreter's: {ctx}"
                );
            }
            0 => {
                t.exited += 1;
                let pc: usize = Mem::get(&me.ci, ci::SAVEDPC);
                assert!(
                    (FAKE_CODE..FAKE_CODE + 4 * c.ncode).contains(&pc),
                    "{who}: savedpc outside the function: {ctx}"
                );
                assert_eq!(
                    [me.reg(0), me.reg(1), me.reg(2)],
                    before,
                    "{who}: registers changed before an exit: {ctx}"
                );
                let d = c.dump(tab);
                assert_eq!(d, pristine, "{who}: table modified before an exit: {ctx}");
            }
            x => panic!("{who}: exit code {x}: {ctx}"),
        }
    };
    check_state(c, &me, before, e_tab, code_e, t, "emulator");
    #[cfg(not(miri))]
    {
        let mut mn = Mem::new(&c.kbytes);
        mn.setup(fnn, &[n_tab, b, cv]);
        let tally_before = (t.completed, t.exited);
        let nbefore = [mn.reg(0), mn.reg(1), mn.reg(2)];
        let code_n = c.run_native(&mut mn);
        assert_eq!(code_e, code_n, "native exit code: {ctx}");
        let mut scratch = Tally::default();
        check_state(c, &mn, nbefore, n_tab, code_n, &mut scratch, "native");
        assert_eq!((t.completed, t.exited), tally_before);
        assert_eq!(
            Mem::get::<usize>(&me.ci, ci::SAVEDPC),
            Mem::get::<usize>(&mn.ci, ci::SAVEDPC),
            "native savedpc: {ctx}"
        );
        assert_eq!(me.cnt, mn.cnt, "native budget: {ctx}");
        // registers: same except pointers to the (different) tables
        for r in 0..3 {
            if mn.reg(r)[0] & 0x3f != u64::from(rn_core::value::LUA_VTABLE) {
                assert_eq!(me.reg(r), mn.reg(r), "native register {r}: {ctx}");
            }
        }
    }
    c.call(4, &[Value::NIL]); // reset anchors
}

fn run_body(body: &str, tabs: &[i64], bs: &[i64], cs: &[i64], gen: bool, min_complete: usize) {
    let mut c = build(body, gen);
    let mut t = Tally::default();
    for &n in tabs {
        for &b in bs {
            for &cc in cs {
                one(&mut c, n, b, cc, &mut t);
            }
        }
    }
    eprintln!(
        "{} [{}]: {} completed, {} exited",
        body,
        if gen { "gen" } else { "inc" },
        t.completed,
        t.exited
    );
    assert!(
        t.completed >= min_complete,
        "`{body}` completes only {} times ({} exits)",
        t.completed,
        t.exited
    );
}

fn all_tabs() -> Vec<i64> {
    (1..=18).collect()
}
fn all_args() -> Vec<i64> {
    (1..=NARGS).collect()
}
const ONE: [i64; 1] = [1];
const VALS: [i64; 10] = [1, 12, 16, 24, 25, 28, 31, 32, 33, 14];

fn gets(gen: bool) {
    // keys in registers / constants
    run_body("return a[b]", &all_tabs(), &all_args(), &ONE, gen, 20);
    for body in [
        "return a[1]",
        "return a[2]",
        "return a[3]",
        "return a[4]",
        "return a[5]",
        // (array limit cases: tables 15-17 have a limit hint below the real array size in 5.4)
        "return a[6]",
        "return a[7]",
        "return a[8]",
        "return a[9]",
        "return a[10]",
        "return a[11]",
        "return a[14]",
        "return a[200]",
        "return a.x",
        "return a.y",
        "return a.name",
        "return a.k7",
        "return a.k1",
        "return a.k24",
        "return a.absent",
        "return T.x",
        "return T.k24",
        "return T.k1",
    ] {
        let min = usize::from(!body.contains("absent"));
        run_body(body, &all_tabs(), &ONE, &ONE, gen, min);
    }
}

fn sets(gen: bool) {
    run_body(
        "a[b] = c; return 0",
        &all_tabs(),
        &all_args(),
        &VALS,
        gen,
        20,
    );
    run_body(
        "a[b] = nil; return 0",
        &all_tabs(),
        &all_args(),
        &ONE,
        gen,
        20,
    );
    run_body(
        "a[b] = b; return 0",
        &all_tabs(),
        &all_args(),
        &ONE,
        gen,
        20,
    );
    for body in [
        "a[1] = c; return 0",
        "a[2] = c; return 0",
        "a[4] = c; return 0",
        "a[6] = c; return 0",
        "a[7] = c; return 0",
        "a[8] = c; return 0",
        "a[9] = c; return 0",
        "a[10] = c; return 0",
        "a[11] = c; return 0",
        "a[3] = 7; return 0",
        "a[2] = 1.5; return 0",
        "a[1] = 'str'; return 0",
        "a[2] = nil; return 0",
        "a[200] = c; return 0",
        "a.x = c; return 0",
        "a.k24 = c; return 0",
        "a.k7 = c; return 0",
        "a.zzz = c; return 0",
        "a.name = 1.5; return 0",
        "a.y = 'strc'; return 0",
        "a.x = nil; return 0",
        "T.x = c; return 0",
        "T.k24 = 5; return 0",
        "T.q = c; return 0",
    ] {
        let min = usize::from(!body.contains("zzz") && !body.contains(".q "));
        run_body(body, &all_tabs(), &ONE, &VALS, gen, min);
    }
}

#[test]
fn gets_inc() {
    gets(false);
}
#[test]
fn gets_gen() {
    gets(true);
}
#[test]
fn sets_inc() {
    sets(false);
}
#[test]
fn sets_gen() {
    sets(true);
}

/// SELF: when the method is found, `R[A]` is the method and `R[A+1]` the table; otherwise nothing changes.
#[test]
fn self_method_lookup() {
    for body in [
        "return a:x()",
        "return a:name()",
        "return a:k24()",
        "return a:absent()",
        "return T:x()",
    ] {
        let must_complete = !body.contains("absent");
        let mut c = build(body, false);
        let selfpc = c
            .code
            .iter()
            .position(|&w| Instruction(w).opcode() == Some(OpCode::SelfOp))
            .expect("SELF");
        let ra = Instruction(c.code[selfpc]).a() as usize;
        let key = body.split(':').nth(1).unwrap().split('(').next().unwrap();
        let rg = format!("return function(t) return canon(rawget(t, '{key}')) end");
        assert_eq!(c.st.load_buffer(rg.as_bytes(), b"=rg", None), Status::Ok);
        let f = c.st.top() - 1;
        assert_eq!(c.st.pcall(f, 1, 0), Status::Ok);
        let rawget_fn = c.st.get(f);
        let mut completed = 0;
        for n in 1..=14 {
            let tab = c.mk(n);
            let clo = c.closure_of(tab);
            let (b, cv) = (c.arg(1), c.arg(1));
            let r = c.call_val(rawget_fn, &[tab]);
            let want = String::from_utf8_lossy(r.as_string().unwrap().as_bytes()).into_owned();
            let mut m = Mem::new(&c.kbytes);
            m.setup(clo, &[tab, b, cv]);
            let before: Vec<[u64; 2]> = (0..=ra + 1).map(|r| m.reg(r)).collect();
            let code = c.emulate(&mut m);
            assert_eq!(code, 0, "stops at the CALL stub or at SELF: {body}");
            let pc: usize = Mem::get(&m.ci, ci::SAVEDPC);
            if (pc - FAKE_CODE) / 4 == selfpc {
                let after: Vec<[u64; 2]> = (0..=ra + 1).map(|r| m.reg(r)).collect();
                if !body.starts_with("return T") {
                    assert_eq!(before, after, "{body}: registers changed on a SELF exit");
                }
                assert_eq!(
                    want, "nil",
                    "{body}: exit although the method exists (table #{n})"
                );
            } else {
                completed += 1;
                let got = c.canon(unraw(m.reg(ra)));
                assert_eq!(got, want, "{body}: method differs (table #{n})");
                assert_eq!(m.reg(ra + 1), raw(tab), "{body}: self argument");
            }
            #[cfg(not(miri))]
            {
                let mut mn = Mem::new(&c.kbytes);
                mn.setup(clo, &[tab, b, cv]);
                let cn = c.run_native(&mut mn);
                assert_eq!(cn, code);
                assert_eq!(mn.frame, m.frame, "{body}: native frame (table #{n})");
            }
            c.call(4, &[Value::NIL]);
        }
        assert!(
            !must_complete || completed > 0,
            "`{body}` never completes SELF"
        );
    }
}

fn mark(v: Value) -> *mut u8 {
    // SAFETY: only called with collectable values (header at the payload pointer).
    unsafe { (raw(v)[1] as *mut u8).add(table::MARKED) }
}

/// The barrier condition: a black table receiving a white collectable value must exit, in every other state the
/// store completes.
#[test]
fn barrier_matrix() {
    let bodies = [
        "a[1] = c; return 0",
        "a[b] = c; return 0",
        "a.x = c; return 0",
        "T.x = c; return 0",
    ];
    // (table marked, value marked): the object colour bits only (white0 = 1<<3, white1 = 1<<4, black = 1<<5)
    let colours: [u8; 4] = [0, 1 << 3, 1 << 4, BLACKBIT];
    for body in bodies {
        let mut c = build(body, false);
        for &tm in &colours {
            for &vm in &colours {
                for vi in [31i64, 32, 33] {
                    // fresh table with array and hash parts populated (values non-nil)
                    let tab = c.mk(4);
                    let v = c.arg(vi);
                    let clo = c.closure_of(tab);
                    let b = c.arg(1);
                    let (old_t, old_v) = {
                        // SAFETY: both are live collectable objects.
                        unsafe { (mark(tab).read(), mark(v).read()) }
                    };
                    // SAFETY: as above; the collector does not run while the emulator does.
                    unsafe {
                        mark(tab).write((old_t & !(BLACKBIT | WHITEBITS)) | tm);
                        mark(v).write((old_v & !(BLACKBIT | WHITEBITS)) | vm);
                    }
                    let mut m = Mem::new(&c.kbytes);
                    m.setup(clo, &[tab, b, v]);
                    let code = c.emulate(&mut m);
                    let need = tm & BLACKBIT != 0 && vm & WHITEBITS != 0;
                    assert_eq!(
                        code == 0,
                        need,
                        "{body}: table mark {tm:#x}, value mark {vm:#x}: barrier needed = {need}, exit code {code}"
                    );
                    #[cfg(not(miri))]
                    {
                        let tab2 = c.mk(4);
                        let clo2 = c.closure_of(tab2);
                        // SAFETY: as above.
                        unsafe {
                            mark(tab2).write((mark(tab2).read() & !(BLACKBIT | WHITEBITS)) | tm);
                        }
                        let mut mn = Mem::new(&c.kbytes);
                        mn.setup(clo2, &[tab2, b, v]);
                        let cn = c.run_native(&mut mn);
                        assert_eq!(cn, code, "{body}: native vs emulator");
                    }
                    // restore sane colours before the collector may run again
                    // SAFETY: as above.
                    unsafe {
                        mark(tab).write(old_t);
                        mark(v).write(old_v);
                    }
                    c.call(4, &[Value::NIL]);
                }
            }
        }
    }
}
