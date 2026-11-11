//! State-based oracle programs (tables and calls): the Lua file runs in a REAL `rn-core` State to build
//! the tables and closures; the hot functions are compiled by the oracle from the State's own `Proto`s and
//! run on a frame the oracle builds (own stack, own `CallInfo` list with the real struct layout from
//! `rn_core::jit_layout`), against the real table objects.
//!
//! File protocol (`bench/jit-oracle/*.lua`): the main chunk defines the hot function(s) first and a
//! `setup` function last, then `if ORACLE then return f, g, ..., setup end` (the oracle defines the global
//! `ORACLE`; `rnlua` does not, so the same file runs as an ordinary program: `print(f(setup(N)))`).
//! `setup(N)` returns the arguments of `f`. All functions before `setup` are compiled (function 0 is the
//! entry point).
use crate::decode::{self, Func};
use crate::fmt_lua_float;
use crate::v1::{self, Cfg, Opts, St, FLT, INT, JP_RESUME};
use crate::v2::{self, PTy, Ty};
use rn_core::jit_layout::{self as lay};
use rn_core::state::{LuaState, LUA_MULTRET};
use rn_core::status::Status;
use rn_core::value::Value;
use rn_jit::mem::{probe_exec, CodeArena};
use std::alloc::{alloc_zeroed, Layout};

type Entry = extern "C" fn(*mut u8, *const u8, *mut St) -> u64;

/// Number of stack slots of the oracle's private Lua stack (1 MiB).
const NSLOTS: usize = 1 << 16;
/// Number of preallocated `CallInfo` nodes (the real list grows on demand; fib depth is below 100).
const NCI: usize = 4096;

fn run_chunk(st: &mut LuaState, code: &[u8], name: &str) -> Vec<Value> {
    let base = st.top();
    if st.load_buffer(code, name.as_bytes(), None) != Status::Ok {
        let msg = st.get(st.top() - 1);
        panic!("load {name}: {msg:?}");
    }
    let func = st.top() - 1;
    finish_call(st, func, base)
}

fn finish_call(
    st: &mut LuaState,
    func: rn_core::state::StkId,
    base: rn_core::state::StkId,
) -> Vec<Value> {
    let status = st.pcall(func, LUA_MULTRET, 0);
    if status != Status::Ok {
        let msg = st.get(st.top() - 1);
        panic!("call failed: {status:?} {msg:?}");
    }
    let vals = st.slice(func, st.top()).to_vec();
    st.set_top(base);
    vals
}

/// Writes `v` at byte offset `off` of the object at `base`.
///
/// # Safety
/// `base + off` must be valid for writing a `T`.
unsafe fn put<T>(base: *mut u8, off: usize, v: T) {
    // SAFETY: per contract.
    unsafe { base.add(off).cast::<T>().write_unaligned(v) }
}

fn value_bits(v: &Value) -> [u64; 2] {
    // SAFETY: `Value` is a 16-byte `repr(C)` pair (tag word, payload) with no padding (jit_layout asserts).
    unsafe { std::mem::transmute_copy::<Value, [u64; 2]>(v) }
}

pub fn run(mode: &str, src: &[u8], n: i64, dump: bool) {
    let (only_compile, mode) = match mode.strip_suffix('c') {
        Some(m) if m.starts_with('v') => (true, m),
        _ => (false, mode),
    };
    let mut st = rn_std::new_state_with_libs().expect("state");
    run_chunk(&mut st, b"collectgarbage('stop')", "=stop");
    let mut text = b"ORACLE=true;".to_vec();
    text.extend_from_slice(src);
    let funcs = run_chunk(&mut st, &text, "=oracle");
    assert!(funcs.len() >= 2, "the file must return f, ..., setup");
    let setup = funcs[funcs.len() - 1];
    let base = st.top();
    let sfunc = st.top();
    st.push(setup);
    st.push(Value::int(n));
    let args = finish_call(&mut st, sfunc, base);

    // the compiled functions
    let ncomp = funcs.len() - 1;
    let gcs: Vec<_> = funcs[..ncomp]
        .iter()
        .map(|v| v.as_lclosure().expect("Lua closure").proto())
        .collect();
    let decoded: Vec<Func> = gcs
        .iter()
        .map(|g| decode::decode_proto(g).expect("decode"))
        .collect();
    if dump {
        for (i, f) in decoded.iter().enumerate() {
            println!("function {i}:");
            for (pc, op) in f.ops.iter().enumerate() {
                println!("{pc:3} {op:?}");
            }
            println!("k = {:?}, max_stack = {}", f.k, f.max_stack);
        }
        return;
    }
    let pl = lay::proto_layout_probe();
    if mode == "v2" {
        run_v2(&decoded[0], &args, &funcs[0], only_compile);
    }
    let cfg = Cfg {
        call_mode: true,
        pl,
    };
    let opts = match mode {
        "v1" => Opts::default(),
        "v1o" => Opts {
            poll: true,
            count: true,
        },
        _ => panic!("unknown mode {mode} for State programs"),
    };
    let refs: Vec<&Func> = decoded.iter().collect();
    let multi = v1::compile_multi(&refs, opts, cfg);

    probe_exec().expect("executable memory");
    let mut arena = CodeArena::new();
    let mut block = arena.alloc(multi.code.len(), 64).expect("alloc");
    arena
        .writer(&mut block)
        .expect("writer")
        .write_at(0, &multi.code);
    arena.commit().expect("commit");
    // SAFETY: the block holds the entry thunk produced by `compile_multi` (System V C ABI: rdi = base,
    // rsi = kcache, rdx = `St`); it lives until the process exits.
    let entry: Entry = unsafe { arena.entry(&block) }.expect("entry");
    let code_base = block.addr() as u64;
    eprintln!(
        "[oracle] {mode}: {} bytes of code for {} functions",
        multi.code.len(),
        ncomp
    );

    // JitProto per compiled proto: [entry][resume address per pc]; its pointer goes into the proto's
    // `gclist` slot (the collector is stopped), emulating the `Proto.jit` field of the real design.
    let mut jps: Vec<Vec<u64>> = Vec::new();
    for (i, f) in decoded.iter().enumerate() {
        let mut jp = vec![0u64; 2 + f.ops.len()];
        jp[0] = code_base + multi.entries[i] as u64;
        for &(pc, off) in &multi.resumes[i] {
            jp[(JP_RESUME as usize) / 8 + pc] = code_base + off as u64;
        }
        jps.push(jp);
    }
    for (g, jp) in gcs.iter().zip(&jps) {
        // SAFETY: `gclist` is a pointer-sized field only used by the (stopped) collector; the pointed-to
        // vector outlives every run and nothing frees the proto before the process exits.
        unsafe {
            put::<*const u64>(g.as_ptr().cast(), lay::proto::GCLIST, jp.as_ptr());
        }
    }
    if only_compile {
        std::process::exit(0);
    }

    // stack, CallInfo list, St
    let mut stack: Vec<[u64; 2]> = vec![[0, 0]; NSLOTS + 8];
    stack[0] = value_bits(&funcs[0]);
    for (i, a) in args.iter().enumerate() {
        stack[1 + i] = value_bits(a);
    }
    let cisz = lay::callinfo::SIZE;
    let layout = Layout::from_size_align(cisz * NCI, 16).unwrap();
    // SAFETY: nonzero size; zeroed memory is a valid `CallInfo` (null pointers, zero integers, `None`).
    let cis = unsafe { alloc_zeroed(layout) };
    assert!(!cis.is_null());
    // SAFETY: every write is inside the `NCI * cisz` block at CallInfo field offsets from `jit_layout`.
    unsafe {
        for i in 0..NCI {
            let c = cis.add(i * cisz);
            if i > 0 {
                put::<*mut u8>(c, lay::callinfo::PREVIOUS, cis.add((i - 1) * cisz));
            }
            if i + 1 < NCI {
                put::<*mut u8>(c, lay::callinfo::NEXT, cis.add((i + 1) * cisz));
            }
        }
        let ci1 = cis.add(cisz);
        put::<usize>(ci1, lay::callinfo::FUNC, 0);
        put::<usize>(ci1, lay::callinfo::TOP, 1 + gcs[0].maxstacksize as usize);
        put::<*const u32>(ci1, lay::callinfo::SAVEDPC, gcs[0].code_ptr());
        put::<*const Value>(ci1, lay::callinfo::KCACHE, gcs[0].k_ptr());
        put::<u32>(
            ci1,
            lay::callinfo::CALLSTATUS,
            2 | lay::callinfo::CIST_FRESH,
        );
    }
    // SAFETY: `cis` holds NCI nodes.
    let ci1 = unsafe { cis.add(cisz) };
    let mut ost = St {
        trap: 0,
        _pad: 0,
        count: i64::MAX,
        abort_pc: 0,
        abort_fn: 0,
        ci: ci1,
        top: 1 + args.len(),
        stack_size: NSLOTS,
        stack_base: stack.as_mut_ptr().cast(),
    };
    let t1 = std::time::Instant::now();
    let bits = entry(
        // SAFETY: slot 1 exists (the stack has NSLOTS + 8 slots).
        unsafe { stack.as_mut_ptr().add(1) }.cast(),
        gcs[0].k_ptr().cast(),
        &mut ost as *mut St,
    );
    let run = t1.elapsed();
    if ost.abort_pc != 0 {
        let (fi, pc) = (ost.abort_fn as usize, ost.abort_pc as usize - 1);
        eprintln!(
            "[oracle] ABORT in function {fi} at pc {pc}: {:?}",
            decoded[fi].ops.get(pc)
        );
        std::process::exit(3);
    }
    eprintln!("[oracle] run {run:?}");
    let tag = stack[0][0] & 0xFF;
    if tag == INT as u64 {
        println!("{}", bits as i64);
    } else if tag == FLT as u64 {
        println!("{}", fmt_lua_float(f64::from_bits(bits)));
    } else {
        println!("unexpected result tag {tag}");
        std::process::exit(4);
    }
    // do not run destructors: the protos' `gclist` slots hold foreign pointers
    use std::io::Write;
    let _ = std::io::stdout().flush();
    std::process::exit(0);
}

/// Element type of the array part of a table (all non-nil elements must share one class).
fn elem_type(t: rn_core::table::TableRef) -> Ty {
    let mut ty = None;
    for i in 1..=i64::from(t.array_size()) {
        let v = t.get_int(i);
        let c = if v.as_int().is_some() {
            Ty::Int
        } else if v.as_float().is_some() {
            Ty::Flt
        } else if v.as_bool().is_some() {
            Ty::Bool
        } else if v.is_nil() {
            continue;
        } else {
            panic!("V2 supports int/float/bool array elements only");
        };
        match ty {
            None => ty = Some(c),
            Some(t0) => assert_eq!(t0, c, "mixed element types in a V2 table parameter"),
        }
    }
    ty.expect("empty table parameter")
}

/// V2-like tier-1.5 upper bound for a call-free function: values in registers, table array pointers and
/// sizes hoisted at entry (whole-function; no spilling), see `v2.rs`.
fn run_v2(f: &Func, args: &[Value], closure: &Value, only_compile: bool) -> ! {
    let params: Vec<PTy> = args
        .iter()
        .map(|v| {
            if v.as_int().is_some() {
                PTy::Int
            } else if v.as_float().is_some() {
                PTy::Flt
            } else if let Some(t) = v.as_table() {
                PTy::Tab(elem_type(t))
            } else {
                panic!("unsupported parameter type for V2: {v:?}")
            }
        })
        .collect();
    let compiled = v2::compile_t(f, &params);
    probe_exec().expect("executable memory");
    let mut arena = CodeArena::new();
    let mut block = arena.alloc(compiled.code.len(), 64).expect("alloc");
    arena
        .writer(&mut block)
        .expect("writer")
        .write_at(0, &compiled.code);
    arena.commit().expect("commit");
    // SAFETY: the block holds a function with the C ABI signature `Entry` produced by the oracle compiler
    // (rdi = frame, rsi = kcache); it lives until the process exits.
    let entry: Entry = unsafe { arena.entry(&block) }.expect("entry");
    eprintln!("[oracle] v2: {} bytes of code", compiled.code.len());
    if only_compile {
        std::process::exit(0);
    }
    let mut frame: Vec<[u64; 2]> = vec![[0, 0]; f.max_stack + 8];
    for (i, a) in args.iter().enumerate() {
        frame[i] = value_bits(a);
    }
    let _ = closure;
    let kp: Vec<[u64; 2]> =
        f.k.iter()
            .map(|k| match *k {
                decode::Kv::Int(i) => [INT as u64, i as u64],
                decode::Kv::Flt(x) => [FLT as u64, x.to_bits()],
                decode::Kv::Bool(b) => [u64::from(if b { 0x11u8 } else { 1u8 }), 0],
                _ => [0, 0],
            })
            .collect();
    let mut ost = St {
        trap: 0,
        _pad: 0,
        count: i64::MAX,
        abort_pc: 0,
        abort_fn: 0,
        ci: std::ptr::null_mut(),
        top: 0,
        stack_size: 0,
        stack_base: std::ptr::null_mut(),
    };
    let t1 = std::time::Instant::now();
    let bits = entry(
        frame.as_mut_ptr().cast(),
        kp.as_ptr().cast(),
        &mut ost as *mut St,
    );
    eprintln!("[oracle] run {:?}", t1.elapsed());
    match compiled.ret_ty {
        Some(t) if t == INT => println!("{}", bits as i64),
        Some(t) if t == FLT => println!("{}", fmt_lua_float(f64::from_bits(bits))),
        other => {
            println!("unexpected result type {other:?}");
            std::process::exit(4);
        }
    }
    use std::io::Write;
    let _ = std::io::stdout().flush();
    std::process::exit(0);
}
