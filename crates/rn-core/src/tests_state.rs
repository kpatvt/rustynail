//! Tests for the State layer: stack, CallInfo, calls, closures/upvalues, to-be-closed variables,
//! metamethod plumbing, threads, userdata, finalizers and the collector integration.
//! (Hand-assembled protos run through the stub executor; end-to-end tests with the real compiler live
//! in `crates/rustynail/tests`.)
#![cfg(not(rustynail_lua54))]
use crate::error::Opnd;
use crate::func::{LClosure, Proto};
use crate::instruction::Instruction;
use crate::meta::Tm;
use crate::protodata::{Const, ProtoData, UpvalDesc as UvDesc};
use crate::state::*;
use crate::status::{LuaResult, Status};
use crate::value::*;
use crate::ver::active::OpCode;
use crate::vm;
use std::cell::RefCell;
use std::rc::Rc;

fn new_state() -> LuaState {
    let mut s = State::with_seed(0x5eed).unwrap();
    s.set_executor(vm::execute);
    s
}

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
fn log(s: String) {
    LOG.with(|l| l.borrow_mut().push(s));
}
fn take_log() -> Vec<String> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

fn ins(op: OpCode, a: u32, b: u32, c: u32) -> u32 {
    Instruction::abck(op, a, b, c, 0).0
}
fn insk(op: OpCode, a: u32, b: u32, c: u32, k: u32) -> u32 {
    Instruction::abck(op, a, b, c, k).0
}
fn insi(op: OpCode, a: u32, sbx: i32) -> u32 {
    Instruction::asbx(op, a, sbx).0
}

fn pdata(code: Vec<u32>, nparams: u8, maxstack: u8) -> ProtoData<crate::string::StrRef> {
    ProtoData {
        num_params: nparams,
        flags: 0,
        max_stack_size: maxstack,
        line_defined: 0,
        last_line_defined: 0,
        code,
        k: vec![],
        protos: vec![],
        upvalues: vec![],
        line_info: vec![],
        abs_line_info: vec![],
        loc_vars: vec![],
        source: None,
    }
}

fn make_closure(st: &mut State, pd: &ProtoData<crate::string::StrRef>) -> Gc<LClosure> {
    let p = Proto::from_data(st.heap(), pd).unwrap();
    let cl = st.heap().new_lclosure(pd.upvalues.len()).unwrap();
    // SAFETY: counts match.
    unsafe { cl.set_proto(p) };
    st.heap().init_upvals(cl).unwrap();
    cl
}

/// Push function + args, call, return the results.
fn call_values(st: &mut State, f: Value, args: &[Value], nres: i32) -> LuaResult<Vec<Value>> {
    let func = st.top();
    st.check_stack(args.len() + 1)?;
    st.push(f);
    for a in args {
        st.push(*a);
    }
    st.call(func, nres)?;
    let res: Vec<Value> = st.slice(func, st.top()).to_vec();
    st.set_top(func);
    Ok(res)
}

fn collect_all(st: &mut State) {
    st.collect(false);
    st.heap().verify();
}

// ---------------------------------------------------------------------------------------------
// basics
// ---------------------------------------------------------------------------------------------

#[test]
fn state_creation_and_registry() {
    let mut s = new_state();
    assert!(s.is_main_thread());
    assert_eq!(s.top(), StkId(1));
    assert_eq!(s.ci(), s.base_ci());
    let reg = s.registry();
    assert!(reg.get_int(LUA_RIDX_GLOBALS).is_table());
    assert!(reg.get_int(LUA_RIDX_MAINTHREAD).is_thread());
    assert_eq!(
        reg.get_int(LUA_RIDX_MAINTHREAD).as_thread(),
        Some(s.as_gc())
    );
    assert!(!s.yieldable()); // main thread is non-yieldable
    assert_eq!(s.ccalls(), 0);
    collect_all(&mut s);
}

#[test]
fn hand_assembled_proto_through_pre_call_and_post_call() {
    let mut s = new_state();
    // function(a, b) local x = 42; return x, a end
    let pd = pdata(
        vec![
            insi(OpCode::Loadi, 2, 42),
            ins(OpCode::Move, 3, 0, 0),
            ins(OpCode::Return, 2, 3, 0), // return R[2], R[3]
        ],
        2,
        4,
    );
    let cl = make_closure(&mut s, &pd);
    let f = Value::lclosure(cl);
    for (nres, want) in [(LUA_MULTRET, 2usize), (0, 0), (1, 1), (2, 2), (4, 4)] {
        let r = call_values(&mut s, f, &[Value::int(7)], nres).unwrap();
        assert_eq!(r.len(), want, "nresults {nres}");
        if want >= 1 {
            assert_eq!(r[0].as_int(), Some(42));
        }
        if want >= 2 {
            assert_eq!(r[1].as_int(), Some(7));
        }
        for v in r.iter().skip(2) {
            assert!(v.is_nil());
        }
        assert_eq!(s.top(), StkId(1));
        assert_eq!(s.ci(), s.base_ci());
    }
    // missing arguments become nil
    let r = call_values(&mut s, f, &[], 2).unwrap();
    assert!(r[1].is_nil());
    collect_all(&mut s);
}

fn n_sum(st: &mut State) -> LuaResult<usize> {
    let mut sum = 0i64;
    for a in st.args() {
        sum += a.as_int().unwrap_or(0);
    }
    st.push(Value::int(sum));
    Ok(1)
}

#[test]
fn native_functions_light_and_closures() {
    let mut s = new_state();
    let r = call_values(
        &mut s,
        Value::native(n_sum),
        &[Value::int(1), Value::int(2), Value::int(3)],
        1,
    )
    .unwrap();
    assert_eq!(r[0].as_int(), Some(6));
    // native closure with upvalues
    let cc = s.heap().new_cclosure(n_sum, 2).unwrap();
    cc.set_upvalue(s.heap(), 1, Value::int(5));
    assert_eq!(cc.upvalue(1).as_int(), Some(5));
    assert!(cc.upvalue(0).is_nil());
    let r = call_values(&mut s, Value::cclosure(cc), &[Value::int(10)], LUA_MULTRET).unwrap();
    assert_eq!(r[0].as_int(), Some(10));
    // calling a non-function: error value on the stack
    let top = s.top();
    s.push(Value::int(3));
    let st = s.pcall(top, 0, 0);
    assert_eq!(st, Status::ErrRun);
    let msg = s.get(top);
    assert_eq!(
        msg.as_string().unwrap().as_bytes(),
        b"attempt to call a number value"
    );
    s.set_top(top);
}

// ---------------------------------------------------------------------------------------------
// stack
// ---------------------------------------------------------------------------------------------

#[test]
fn stack_growth_keeps_values_and_open_upvalues() {
    let mut s = new_state();
    let g0 = s.stack_gen();
    for i in 0..30 {
        s.push_checked(Value::int(i)).unwrap();
    }
    let uv = s.find_upval(StkId(11)).unwrap();
    assert!(uv.is_open());
    assert_eq!(uv.get().as_int(), Some(10));
    uv.set(Value::int(1000));
    assert_eq!(s.get(StkId(11)).as_int(), Some(1000)); // upvalue writes through to the stack
    s.check_stack(200_000).unwrap();
    if cfg!(debug_assertions) {
        assert_ne!(s.stack_gen(), g0);
    }
    assert!(s.stack_size() >= 200_000);
    assert_eq!(uv.get().as_int(), Some(1000)); // pointer was re-derived
    s.set(StkId(11), Value::int(-5));
    assert_eq!(uv.get().as_int(), Some(-5));
    for i in 1..30 {
        if i != 10 {
            assert_eq!(s.get(StkId(i + 1)).as_int(), Some(i as i64), "slot {i}");
        }
    }
    // shrink back
    s.set_top(StkId(31));
    s.shrink_stack();
    assert!(s.stack_size() < 200_000);
    assert_eq!(uv.get().as_int(), Some(-5));
    s.close_upvals(StkId(0));
    assert!(!uv.is_open());
    assert_eq!(uv.get().as_int(), Some(-5));
    s.set_top(StkId(1));
    collect_all(&mut s);
}

#[test]
fn stack_overflow_is_an_error_and_recovers() {
    let mut s = new_state();
    let top = s.top();
    let st = s.pcall_with(
        |s| loop {
            s.check_stack(1000)?;
            let t = s.top();
            s.set_top(t + 1000);
        },
        top,
        0,
    );
    assert_eq!(st, Status::ErrRun);
    assert_eq!(
        s.get(top).as_string().unwrap().as_bytes(),
        b"stack overflow"
    );
    assert_eq!(s.top(), top + 1);
    assert!(s.stack_size() <= LUAI_MAXSTACK, "stack shrunk after error");
    s.set_top(top);
    // usable afterwards
    s.check_stack(5000).unwrap();
}

#[test]
fn memory_error_in_stack_growth_is_recoverable() {
    let mut s = new_state();
    let lim = s.heap().total_bytes() + 4000;
    s.heap().mem_mut().set_limit(lim);
    let top = s.top();
    let st = s.pcall_with(|s| s.check_stack(100_000).map(|_| ()), top, 0);
    assert_eq!(st, Status::ErrMem);
    assert_eq!(
        s.get(top).as_string().unwrap().as_bytes(),
        MEMERRMSG,
        "preallocated message"
    );
    s.heap().mem_mut().set_limit(usize::MAX);
    s.set_top(top);
    s.check_stack(100_000).unwrap();
}

fn rec(st: &mut State) -> LuaResult<usize> {
    let f = st.top();
    st.check_stack(1)?;
    st.push(Value::native(rec));
    st.call(f, 0)?;
    Ok(0)
}

#[test]
fn c_stack_overflow_and_error_in_error_handling() {
    let mut s = new_state();
    let top = s.top();
    s.push(Value::native(rec));
    let st = s.pcall(top, 0, 0);
    assert_eq!(st, Status::ErrRun);
    assert_eq!(
        s.get(top).as_string().unwrap().as_bytes(),
        b"C stack overflow"
    );
    s.set_top(top);
    assert_eq!(s.ccalls(), 0);
    assert_eq!(s.ci(), s.base_ci());
    // a message handler that overflows too -> ErrErr
    s.push(Value::native(rec)); // handler at `top`
    let h = s.top() - 1;
    s.push(Value::native(rec));
    let f = s.top() - 1;
    let st = s.pcall(f, 0, h.idx());
    assert_eq!(st, Status::ErrErr);
    assert_eq!(
        s.get(f).as_string().unwrap().as_bytes(),
        b"error in error handling"
    );
    s.set_top(top);
}

#[test]
fn callinfo_list_grows_and_shrinks() {
    let mut s = new_state();
    assert_eq!(s.nci(), 0);
    fn deep(st: &mut State) -> LuaResult<usize> {
        let n = st.arg(0).as_int().unwrap();
        if n > 0 {
            let f = st.top();
            st.push(Value::native(deep));
            st.push(Value::int(n - 1));
            st.call(f, 0)?;
        }
        Ok(0)
    }
    let r = call_values(&mut s, Value::native(deep), &[Value::int(50)], 0);
    assert!(r.is_ok());
    assert!(s.nci() >= 50, "nci = {}", s.nci());
    for _ in 0..8 {
        s.shrink_stack();
    }
    assert!(s.nci() <= 2);
    assert_eq!(s.ci(), s.base_ci());
}

// ---------------------------------------------------------------------------------------------
// closures and upvalues
// ---------------------------------------------------------------------------------------------

#[test]
fn upvalue_lifecycle_shared_by_two_closures() {
    let mut s = new_state();
    for _ in 0..5 {
        s.push_checked(Value::int(0)).unwrap();
    }
    let slot = StkId(3);
    s.set(slot, Value::int(1));
    let uv1 = s.find_upval(slot).unwrap();
    let uv2 = s.find_upval(slot).unwrap();
    assert_eq!(uv1, uv2);
    let other = s.find_upval(StkId(4)).unwrap();
    assert_ne!(uv1, other);
    // two closures share the upvalue
    let mk = |s: &mut State| {
        let pd = ProtoData {
            upvalues: vec![UvDesc {
                name: None,
                in_stack: true,
                idx: 0,
                kind: 0,
            }],
            ..pdata(vec![ins(OpCode::Return0, 0, 0, 0)], 0, 2)
        };
        make_closure(s, &pd)
    };
    let c1 = mk(&mut s);
    let c2 = mk(&mut s);
    c1.set_upval(s.heap(), 0, uv1);
    c2.set_upval(s.heap(), 0, uv2);
    s.push_checked(Value::lclosure(c1)).unwrap();
    s.push_checked(Value::lclosure(c2)).unwrap();
    c1.upval(0).set(Value::int(99));
    assert_eq!(s.get(slot).as_int(), Some(99));
    assert_eq!(c2.upval(0).get().as_int(), Some(99));
    // close: the value moves into the upvalue, stack writes no longer visible
    s.close_upvals(slot);
    assert!(!uv1.is_open());
    assert!(!other.is_open());
    assert_eq!(c2.upval(0).get().as_int(), Some(99));
    s.set(slot, Value::int(-1));
    assert_eq!(c1.upval(0).get().as_int(), Some(99));
    c2.upval(0).set(Value::int(7));
    assert_eq!(c1.upval(0).get().as_int(), Some(7));
    // a new open upvalue for the same slot is a fresh object
    let uv3 = s.find_upval(slot).unwrap();
    assert_ne!(uv3, uv1);
    s.close_upvals(StkId(0));
    // closures survive a collection with their (closed) upvalues
    collect_all(&mut s);
    assert_eq!(c1.upval(0).get().as_int(), Some(7));
    s.set_top(StkId(1));
    collect_all(&mut s);
}

#[test]
fn closures_and_protos_are_traversed_and_freed() {
    let mut s = new_state();
    collect_all(&mut s);
    let base = s.g().heap.object_count();
    let sbytes = s.heap().total_bytes();
    let nested = pdata(vec![ins(OpCode::Return0, 0, 0, 0)], 0, 2);
    let mut pd = pdata(vec![ins(OpCode::Return0, 0, 0, 0)], 0, 2);
    pd.protos = vec![nested.clone(), nested];
    let name = s.new_str(b"a-fairly-unique-constant-string").unwrap();
    pd.k = vec![
        Const::Str(name),
        Const::Int(5),
        Const::Float(1.5),
        Const::Nil,
        Const::True,
    ];
    let cl = make_closure(&mut s, &pd);
    assert_eq!(cl.proto().protos().len(), 2);
    assert_eq!(cl.proto().k().len(), 5);
    s.push_checked(Value::lclosure(cl)).unwrap();
    collect_all(&mut s);
    assert_eq!(
        cl.proto().k()[0].as_string().unwrap().as_bytes(),
        b"a-fairly-unique-constant-string"
    );
    s.set_top(StkId(1));
    collect_all(&mut s);
    assert_eq!(s.g().heap.object_count(), base);
    assert!(s.heap().total_bytes() <= sbytes);
}

// ---------------------------------------------------------------------------------------------
// __call
// ---------------------------------------------------------------------------------------------

fn n_count_args(st: &mut State) -> LuaResult<usize> {
    let n = st.nargs();
    let first = st.arg(0);
    log(format!(
        "args={n} first_is_table={} last={:?}",
        first.is_table(),
        st.args().last().and_then(|v| v.as_int())
    ));
    Ok(0)
}

fn set_call_mm(st: &mut State, obj: Value, f: Value) {
    let t = obj.as_table().unwrap();
    let mt = st.new_table().unwrap();
    let k = st.new_str_value(b"__call").unwrap();
    st.raw_set(mt, &k, &f).unwrap();
    st.set_metatable(&Value::table(t), Some(mt));
}

#[test]
fn call_metamethod_chain() {
    let mut s = new_state();
    take_log();
    let t1 = Value::table(s.new_table().unwrap());
    let t2 = Value::table(s.new_table().unwrap());
    set_call_mm(&mut s, t2, Value::native(n_count_args));
    set_call_mm(&mut s, t1, t2);
    s.push_checked(t1).unwrap(); // anchor
    s.push_checked(t2).unwrap();
    call_values(&mut s, t1, &[Value::int(1), Value::int(2)], 0).unwrap();
    // args: (t2, t1, 1, 2)
    assert_eq!(take_log(), vec!["args=4 first_is_table=true last=Some(2)"]);
    // an overlong chain is reported
    let mut prev = Value::native(n_count_args);
    for _ in 0..20 {
        let t = Value::table(s.new_table().unwrap());
        s.push_checked(t).unwrap();
        set_call_mm(&mut s, t, prev);
        prev = t;
    }
    let top = s.top();
    s.push_checked(prev).unwrap();
    let st = s.pcall(top, 0, 0);
    assert_eq!(st, Status::ErrRun);
    assert_eq!(
        s.get(top).as_string().unwrap().as_bytes(),
        b"'__call' chain too long"
    );
    s.set_top(StkId(1));
}

// ---------------------------------------------------------------------------------------------
// metamethod lookup and flags
// ---------------------------------------------------------------------------------------------

#[test]
fn metamethod_cache_flags_are_invalidated_by_writes() {
    let mut s = new_state();
    let mt = s.new_table().unwrap();
    s.push_checked(Value::table(mt)).unwrap();
    assert!(s.fast_tm(Some(mt), Tm::Index).is_none());
    assert_ne!(mt.flags() & (1 << Tm::Index as u8), 0, "absence cached");
    let key = s.new_str_value(b"__index").unwrap();
    s.raw_set(mt, &key, &Value::int(1)).unwrap();
    assert_eq!(mt.flags() & (1 << Tm::Index as u8), 0, "cache invalidated");
    assert_eq!(s.fast_tm(Some(mt), Tm::Index).unwrap().as_int(), Some(1));
    // per-type metatables
    let t = Value::table(s.new_table().unwrap());
    assert!(s.get_tm_by_obj(&Value::int(1), Tm::Add).is_nil());
    let nmt = s.new_table().unwrap();
    s.push_checked(Value::table(nmt)).unwrap();
    let k = s.new_str_value(b"__add").unwrap();
    s.raw_set(nmt, &k, &Value::native(n_sum)).unwrap();
    s.set_type_metatable(LuaType::Number, Some(nmt));
    assert!(s.get_tm_by_obj(&Value::float(1.0), Tm::Add).is_function());
    assert!(s.get_tm_by_obj(&t, Tm::Add).is_nil());
    // binary metamethod through try_bin_tm: 1 + 2 with __add on numbers
    let res = s.top();
    s.push_checked(Value::NIL).unwrap();
    s.try_bin_tm(Value::int(3).into(), Value::int(4).into(), res, Tm::Add)
        .unwrap();
    assert_eq!(s.get(res).as_int(), Some(7));
    s.set_type_metatable(LuaType::Number, None);
    // no metamethod: error message
    let top = s.top();
    let st = s.pcall_with(
        |s| {
            s.try_bin_tm(
                Opnd::from(Value::NIL),
                Opnd::from(Value::int(1)),
                res,
                Tm::Add,
            )
        },
        top,
        0,
    );
    assert_eq!(st, Status::ErrRun);
    assert_eq!(
        s.get(top).as_string().unwrap().as_bytes(),
        b"attempt to perform arithmetic on a nil value"
    );
    s.set_top(StkId(1));
}

#[test]
fn obj_typename_uses_name_metafield() {
    let mut s = new_state();
    let t = s.new_table().unwrap();
    s.push_checked(Value::table(t)).unwrap();
    assert_eq!(s.obj_typename(&Value::table(t)), b"table");
    let mt = s.new_table().unwrap();
    s.push_checked(Value::table(mt)).unwrap();
    let k = s.new_str_value(b"__name").unwrap();
    let v = s.new_str_value(b"MyType").unwrap();
    s.raw_set(mt, &k, &v).unwrap();
    s.set_metatable(&Value::table(t), Some(mt));
    assert_eq!(s.obj_typename(&Value::table(t)), b"MyType");
    assert_eq!(s.obj_typename(&Value::int(1)), b"number");
    assert_eq!(s.obj_typename(&Value::NIL), b"nil");
    let u = s.new_udata(4, 0).unwrap();
    assert_eq!(s.obj_typename(&Value::userdata(u)), b"userdata");
    s.set_top(StkId(1));
}

// ---------------------------------------------------------------------------------------------
// to-be-closed variables
// ---------------------------------------------------------------------------------------------

fn n_closer(st: &mut State) -> LuaResult<usize> {
    let obj = st.arg(0);
    let err = st.arg(1);
    let nm = st.new_str(b"name")?;
    let name = obj.as_table().unwrap().get_str(nm);
    let name = String::from_utf8_lossy(name.as_string().unwrap().as_bytes()).into_owned();
    let errs = match err.as_string() {
        Some(e) => String::from_utf8_lossy(e.as_bytes()).into_owned(),
        None => "nil".into(),
    };
    log(format!("close {name} err={errs}"));
    // an object called "bad" raises an error while closing
    if name == "bad" {
        let m = st.new_str_value(b"second")?;
        return Err(st.throw_value(m));
    }
    Ok(0)
}

fn make_tbc_obj(st: &mut State, name: &str) -> Value {
    let t = st.new_table().unwrap();
    st.push(Value::table(t)); // anchor
    let mt = st.new_table().unwrap();
    st.push(Value::table(mt));
    let k = st.new_str_value(b"__close").unwrap();
    st.raw_set(mt, &k, &Value::native(n_closer)).unwrap();
    let nk = st.new_str_value(b"name").unwrap();
    let nv = st.new_str_value(name.as_bytes()).unwrap();
    st.raw_set(t, &nk, &nv).unwrap();
    st.set_metatable(&Value::table(t), Some(mt));
    st.pop();
    st.pop();
    Value::table(t)
}

#[test]
fn tbc_close_order_and_error_chain() {
    let mut s = new_state();
    take_log();
    // normal close, in reverse order; `false` and `nil` need no closing
    let base = s.top();
    for n in ["a", "b", "c"] {
        let v = make_tbc_obj(&mut s, n);
        let slot = s.top();
        s.push_checked(v).unwrap();
        s.new_tbc_upval(slot).unwrap();
    }
    let slot = s.top();
    s.push_checked(Value::FALSE).unwrap();
    s.new_tbc_upval(slot).unwrap();
    assert_eq!(s.tbc_count(), 3);
    s.close(base, Status::Ok as u8, false).unwrap();
    assert_eq!(
        take_log(),
        vec!["close c err=nil", "close b err=nil", "close a err=nil"]
    );
    assert_eq!(s.tbc_count(), 0);
    s.set_top(base);

    // non-closable value
    let top = s.top();
    let st = s.pcall_with(
        |s| {
            let slot = s.top();
            s.push_checked(Value::int(1))?;
            s.new_tbc_upval(slot)
        },
        top,
        0,
    );
    assert_eq!(st, Status::ErrRun);
    let m = String::from_utf8_lossy(s.get(top).as_string().unwrap().as_bytes()).into_owned();
    assert!(m.starts_with("variable "), "{m}");
    assert!(m.ends_with("got a non-closable value"), "{m}");
    s.set_top(base);

    // error while running: every pending variable is closed with the error; an error in a
    // closing method replaces the error object for the remaining ones
    let top = s.top();
    let st = s.pcall_with(
        |s| {
            for n in ["one", "bad", "three"] {
                let v = make_tbc_obj(s, n);
                let slot = s.top();
                s.push_checked(v)?;
                s.new_tbc_upval(slot)?;
            }
            let m = s.new_str_value(b"boom")?;
            Err(s.throw_value(m))
        },
        top,
        0,
    );
    assert_eq!(st, Status::ErrRun);
    assert_eq!(s.get(top).as_string().unwrap().as_bytes(), b"second");
    assert_eq!(
        take_log(),
        vec![
            "close three err=boom",
            "close bad err=boom",
            "close one err=second"
        ]
    );
    assert_eq!(s.tbc_count(), 0);
    assert_eq!(s.ci(), s.base_ci());
    s.set_top(base);
    collect_all(&mut s);
}

// ---------------------------------------------------------------------------------------------
// message handler / error values
// ---------------------------------------------------------------------------------------------

fn n_handler(st: &mut State) -> LuaResult<usize> {
    let m = st.arg(0);
    let mut b = b"handled: ".to_vec();
    if let Some(s) = m.as_string() {
        b.extend_from_slice(s.as_bytes());
    }
    let v = st.new_str_value(&b)?;
    st.push(v);
    Ok(1)
}

fn n_thrower(st: &mut State) -> LuaResult<usize> {
    let v = st.new_str_value(b"oops")?;
    Err(st.throw_value(v))
}

#[test]
fn message_handler_and_error_values() {
    let mut s = new_state();
    let h = s.top();
    s.push_checked(Value::native(n_handler)).unwrap();
    let f = s.top();
    s.push_checked(Value::native(n_thrower)).unwrap();
    let st = s.pcall(f, 0, h.idx());
    assert_eq!(st, Status::ErrRun);
    assert_eq!(s.get(f).as_string().unwrap().as_bytes(), b"handled: oops");
    s.set_top(h);
    // non-string error objects are preserved; nil becomes "<no error object>" in 5.5
    fn throw_tbl(st: &mut State) -> LuaResult<usize> {
        let t = st.new_table()?;
        Err(st.throw_value(Value::table(t)))
    }
    let f = s.top();
    s.push_checked(Value::native(throw_tbl)).unwrap();
    assert_eq!(s.pcall(f, 0, 0), Status::ErrRun);
    assert!(s.get(f).is_table());
    s.set_top(f);
    fn throw_nil(st: &mut State) -> LuaResult<usize> {
        Err(st.throw_value(Value::NIL))
    }
    s.push_checked(Value::native(throw_nil)).unwrap();
    assert_eq!(s.pcall(f, 0, 0), Status::ErrRun);
    assert_eq!(
        s.get(f).as_string().unwrap().as_bytes(),
        b"<no error object>"
    );
    s.set_top(f);
}

// ---------------------------------------------------------------------------------------------
// userdata
// ---------------------------------------------------------------------------------------------

#[test]
fn userdata_layout_alignment_and_user_values() {
    let mut s = new_state();
    for nuv in [0usize, 1, 2, 7] {
        for len in [0usize, 1, 5, 16, 100] {
            let u = s.new_udata(len, nuv).unwrap();
            s.push_checked(Value::userdata(u)).unwrap();
            assert_eq!(u.data_ptr() as usize % 16, 0, "nuv={nuv} len={len}");
            assert_eq!(u.len(), len);
            // payload is zero-initialised and writable
            // SAFETY: exclusive access in this test.
            unsafe {
                assert!(u.data().iter().all(|&b| b == 0));
                for (i, b) in u.data_mut().iter_mut().enumerate() {
                    *b = i as u8;
                }
            }
            for n in 0..nuv {
                let v = s.new_str_value(format!("uv{n}").as_bytes()).unwrap();
                assert!(u.set_user_value(s.heap(), n, v));
            }
            assert!(!u.set_user_value(s.heap(), nuv, Value::NIL));
            collect_all(&mut s);
            for n in 0..nuv {
                assert_eq!(
                    u.user_value(n).unwrap().as_string().unwrap().as_bytes(),
                    format!("uv{n}").as_bytes()
                );
            }
            // SAFETY: as above.
            unsafe {
                for (i, b) in u.data().iter().enumerate() {
                    assert_eq!(*b, i as u8);
                }
            }
        }
    }
    s.set_top(StkId(1));
    let before = s.heap().total_bytes();
    collect_all(&mut s);
    assert!(s.heap().total_bytes() < before);
}

// ---------------------------------------------------------------------------------------------
// threads
// ---------------------------------------------------------------------------------------------

#[test]
fn thread_creation_extra_space_and_independent_stacks() {
    let mut s = new_state();
    // SAFETY: the extra space is LUA_EXTRASPACE bytes in front of the thread.
    unsafe {
        let p = s.extra_space();
        for i in 0..LUA_EXTRASPACE {
            p.add(i).write(0xA0 + i as u8);
        }
    }
    let th = s.new_thread().unwrap();
    assert_eq!(s.pop().as_thread(), Some(th));
    // SAFETY: single user of the handle.
    let mut t = unsafe { State::from_gc(th) };
    assert!(!t.is_main_thread());
    assert_eq!(t.top(), StkId(1));
    // SAFETY: as above.
    unsafe {
        let p = t.extra_space();
        for i in 0..LUA_EXTRASPACE {
            assert_eq!(*p.add(i), 0xA0 + i as u8);
        }
    }
    // different stack, same globals
    t.push_checked(Value::int(5)).unwrap();
    assert_eq!(s.top(), StkId(1));
    assert_eq!(t.top(), StkId(2));
    assert_eq!(t.globals(), s.globals());
    // a call on the coroutine thread runs on its own stack
    let r = call_values(
        &mut t,
        Value::native(n_sum),
        &[Value::int(2), Value::int(3)],
        1,
    )
    .unwrap();
    assert_eq!(r[0].as_int(), Some(5));
    // unreferenced threads are collected
    collect_all(&mut s);
    let base = s.g().heap.object_count();
    for _ in 0..200 {
        let th = s.new_thread().unwrap();
        // SAFETY: single user.
        let mut t = unsafe { State::from_gc(th) };
        for i in 0..100 {
            t.push_checked(Value::int(i)).unwrap();
        }
        s.pop();
    }
    collect_all(&mut s);
    assert_eq!(s.g().heap.object_count(), base);
}

#[test]
fn dead_thread_open_upvalues_are_closed_when_the_closure_survives() {
    let mut s = new_state();
    let keep = {
        let th = s.new_thread().unwrap();
        // SAFETY: single user.
        let mut t = unsafe { State::from_gc(th) };
        t.push_checked(Value::int(41)).unwrap();
        let uv = t.find_upval(StkId(1)).unwrap();
        assert!(uv.is_open());
        uv.set(Value::int(42));
        // the thread must be in the global twups list now
        assert!(t.t().twups != t.th_ptr());
        let pd = ProtoData {
            upvalues: vec![UvDesc {
                name: None,
                in_stack: true,
                idx: 0,
                kind: 0,
            }],
            ..pdata(vec![ins(OpCode::Return0, 0, 0, 0)], 0, 2)
        };
        let cl = make_closure(&mut s, &pd);
        cl.set_upval(s.heap(), 0, uv);
        s.pop(); // drop the thread
        cl
    };
    s.push_checked(Value::lclosure(keep)).unwrap();
    collect_all(&mut s); // thread dies; upvalue must close with its value
    assert!(!keep.upval(0).is_open());
    assert_eq!(keep.upval(0).get().as_int(), Some(42));
    collect_all(&mut s);
    s.set_top(StkId(1));
    collect_all(&mut s);
}

#[test]
fn live_thread_with_open_upvalue_keeps_stack_values_alive() {
    let mut s = new_state();
    let th = s.new_thread().unwrap();
    // SAFETY: single user.
    let mut t = unsafe { State::from_gc(th) };
    let v = t
        .new_str_value(b"a string living only in the coroutine stack")
        .unwrap();
    t.push_checked(v).unwrap();
    let uv = t.find_upval(StkId(1)).unwrap();
    for _ in 0..5 {
        collect_all(&mut s); // thread anchored on s's stack
    }
    assert_eq!(
        uv.get().as_string().unwrap().as_bytes(),
        b"a string living only in the coroutine stack"
    );
    // unreferenced upvalue of a live thread: also fine
    s.pop();
    t.set_top(StkId(1));
    s.heap().verify();
}

// ---------------------------------------------------------------------------------------------
// GC integration, finalizers
// ---------------------------------------------------------------------------------------------

fn n_gc_logger(st: &mut State) -> LuaResult<usize> {
    let obj = st.arg(0);
    let nm = st.new_str(b"name")?;
    let name = obj.as_table().unwrap().get_str(nm);
    log(format!(
        "gc {}",
        String::from_utf8_lossy(name.as_string().unwrap().as_bytes())
    ));
    // resurrection: objects named "res*" are stored in the registry
    if name.as_string().unwrap().as_bytes().starts_with(b"res") {
        let reg = st.registry();
        let k = st.new_str_value(b"resurrected")?;
        st.raw_set(reg, &k, &obj)?;
    }
    if name.as_string().unwrap().as_bytes() == b"err" {
        let m = st.new_str_value(b"finalizer error")?;
        return Err(st.throw_value(m));
    }
    Ok(0)
}

fn make_gc_obj(st: &mut State, name: &str) -> Value {
    let t = st.new_table().unwrap();
    st.push(Value::table(t));
    let mt = st.new_table().unwrap();
    st.push(Value::table(mt));
    let k = st.new_str_value(b"__gc").unwrap();
    st.raw_set(mt, &k, &Value::native(n_gc_logger)).unwrap();
    let nk = st.new_str_value(b"name").unwrap();
    let nv = st.new_str_value(name.as_bytes()).unwrap();
    st.raw_set(t, &nk, &nv).unwrap();
    st.set_metatable(&Value::table(t), Some(mt));
    st.pop();
    st.pop();
    Value::table(t)
}

#[test]
fn finalizers_run_in_reverse_order_resurrect_and_run_once() {
    let mut s = new_state();
    collect_all(&mut s);
    take_log();
    for n in ["a", "b", "c"] {
        make_gc_obj(&mut s, n);
    }
    collect_all(&mut s);
    assert_eq!(take_log(), vec!["gc c", "gc b", "gc a"]);
    collect_all(&mut s);
    assert!(take_log().is_empty(), "finalized only once");
    // resurrection: the finalizer stores the object; it is not finalized again
    make_gc_obj(&mut s, "res1");
    collect_all(&mut s);
    assert_eq!(take_log(), vec!["gc res1"]);
    let reg = s.registry();
    let k = s.new_str_value(b"resurrected").unwrap();
    assert!(reg.get(&k).is_table());
    reg.set(s.heap(), &k, &Value::NIL).unwrap();
    collect_all(&mut s);
    assert!(take_log().is_empty());
    collect_all(&mut s);
}

#[test]
fn finalizer_errors_become_warnings() {
    let mut s = new_state();
    let warnings: Rc<RefCell<Vec<String>>> = Rc::default();
    let w = warnings.clone();
    s.set_warn_function(Some(Box::new(move |m: &[u8], cont: bool| {
        w.borrow_mut().push(format!(
            "{}{}",
            String::from_utf8_lossy(m),
            if cont { "+" } else { "." }
        ));
    })));
    take_log();
    make_gc_obj(&mut s, "err");
    let top = s.top();
    collect_all(&mut s);
    assert_eq!(s.top(), top, "error object popped");
    assert_eq!(take_log(), vec!["gc err"]);
    let joined: String = warnings.borrow().concat();
    assert_eq!(joined, "error in +__gc+ (+finalizer error+).");
    assert!(s.allow_hook());
}

#[test]
fn close_state_runs_pending_finalizers_and_tbc() {
    take_log();
    {
        let mut s = new_state();
        for n in ["x", "y"] {
            let o = make_gc_obj(&mut s, n);
            s.push_checked(o).unwrap(); // stay reachable until close
        }
        // a pending to-be-closed variable in the main thread
        let o = make_tbc_obj(&mut s, "tbc");
        let slot = s.top();
        s.push_checked(o).unwrap();
        s.new_tbc_upval(slot).unwrap();
        assert!(take_log().is_empty());
    } // drop closes the state
    assert_eq!(
        take_log(),
        vec!["close tbc err=nil", "gc y", "gc x"],
        "tbc first, then finalizers in reverse order"
    );
}

#[test]
fn gc_stress_workload_with_threads_closures_userdata() {
    let mut s = new_state();
    s.heap().set_gc_stress(true);
    let n = if cfg!(miri) { 12 } else { 200 };
    for round in 0..n {
        let top = s.top();
        // a table holding closures, userdata and a thread
        let t = s.new_table().unwrap();
        s.push_checked(Value::table(t)).unwrap();
        for i in 0..4 {
            let pd = ProtoData {
                upvalues: vec![UvDesc {
                    name: None,
                    in_stack: false,
                    idx: 0,
                    kind: 0,
                }],
                k: vec![],
                ..pdata(vec![ins(OpCode::Return0, 0, 0, 0)], 0, 2)
            };
            let cl = make_closure(&mut s, &pd);
            s.push_checked(Value::lclosure(cl)).unwrap();
            s.check_gc();
            let u = s.new_udata(8 + i, 1).unwrap();
            s.push_checked(Value::userdata(u)).unwrap();
            let name = s
                .new_str_value(format!("name{round}_{i}").as_bytes())
                .unwrap();
            u.set_user_value(s.heap(), 0, name);
            s.raw_set_int(t, i as i64 + 1, &Value::lclosure(cl))
                .unwrap();
            s.raw_set_int(t, i as i64 + 10, &Value::userdata(u))
                .unwrap();
            s.set_top(top + 1);
        }
        let th = s.new_thread().unwrap();
        s.raw_set_int(t, 100, &Value::thread(th)).unwrap();
        // SAFETY: single user.
        let mut ts = unsafe { State::from_gc(th) };
        ts.push_checked(Value::table(t)).unwrap();
        let uv = ts.find_upval(StkId(1)).unwrap();
        s.check_gc();
        assert!(uv.get().is_table());
        s.set_top(top + 1);
        s.check_gc();
        // contents survive
        for i in 0..4 {
            assert!(t.get_int(i + 1).is_function());
            let u = t.get_int(i + 10).as_userdata().unwrap();
            assert_eq!(
                u.user_value(0).unwrap().as_string().unwrap().as_bytes(),
                format!("name{round}_{i}").as_bytes()
            );
        }
        s.set_top(top);
    }
    s.heap().set_gc_stress(false);
    collect_all(&mut s);
}

// ---------------------------------------------------------------------------------------------
// loader hook
// ---------------------------------------------------------------------------------------------

fn fake_loader(
    heap: &mut crate::heap::Heap,
    chunk: &[u8],
    name: &[u8],
    mode: &[u8],
) -> Result<ProtoData<crate::string::StrRef>, Vec<u8>> {
    if chunk.starts_with(b"bad") {
        let mut m = name.to_vec();
        m.extend_from_slice(b": syntax error (mode ");
        m.extend_from_slice(mode);
        m.push(b')');
        return Err(m);
    }
    let src = heap.new_str(name).unwrap();
    let mut pd = pdata(
        vec![insi(OpCode::Loadi, 0, 7), ins(OpCode::Return1, 0, 2, 0)],
        0,
        2,
    );
    pd.flags = crate::protodata::PF_VAHID;
    pd.source = Some(src);
    pd.upvalues = vec![UvDesc {
        name: Some(heap.new_str(b"_ENV").unwrap()),
        in_stack: true,
        idx: 0,
        kind: 0,
    }];
    Ok(pd)
}

#[test]
fn load_chunk_sets_env_upvalue_and_reports_errors() {
    let mut s = new_state();
    s.set_loader(fake_loader);
    let top = s.top();
    assert_eq!(s.load_chunk(b"good", b"=chunk", None), Status::Ok);
    assert_eq!(s.top(), top + 1);
    let cl = s.get(top).as_lclosure().unwrap();
    assert_eq!(cl.nupvalues(), 1);
    assert_eq!(cl.upval(0).get().as_table(), Some(s.globals()));
    assert_eq!(cl.proto().source().unwrap().get().as_bytes(), b"=chunk");
    assert_eq!(
        cl.proto().upvalue_descs()[0]
            .name()
            .unwrap()
            .get()
            .as_bytes(),
        b"_ENV"
    );
    s.set_top(top);
    assert_eq!(s.load_chunk(b"bad", b"=x", Some(b"t")), Status::ErrSyntax);
    assert_eq!(s.top(), top + 1);
    assert_eq!(
        s.get(top).as_string().unwrap().as_bytes(),
        b"=x: syntax error (mode t)"
    );
    s.set_top(top);
    assert!(!s.yieldable());
    assert_eq!(s.ncalls_raw(), 0x10000, "nny restored");
    // without a loader
    let mut s2 = new_state();
    assert_eq!(s2.load_chunk(b"x", b"=x", None), Status::ErrSyntax);
}

// ---------------------------------------------------------------------------------------------
// vararg helpers (hand-assembled vararg function)
// ---------------------------------------------------------------------------------------------

#[test]
fn vararg_function_through_stub_executor() {
    let mut s = new_state();
    // function(a, ...) return a, ... end
    let mut pd = pdata(
        vec![
            ins(OpCode::Varargprep, 1, 0, 0),
            ins(OpCode::Move, 1, 0, 0),
            ins(OpCode::Vararg, 2, 0, 0),
            insk(OpCode::Return, 1, 0, 2, 0),
        ],
        1,
        6,
    );
    pd.flags = crate::protodata::PF_VAHID;
    // VARARGPREP A=nparams; VARARG A C (C=0 -> multret); RETURN A B=0 C=nparams+1
    let cl = make_closure(&mut s, &pd);
    let args: Vec<Value> = (1..=5).map(Value::int).collect();
    let r = call_values(&mut s, Value::lclosure(cl), &args, LUA_MULTRET).unwrap();
    let got: Vec<i64> = r.iter().map(|v| v.as_int().unwrap()).collect();
    assert_eq!(got, vec![1, 2, 3, 4, 5]);
    assert_eq!(s.top(), StkId(1));
    // no extra arguments
    let r = call_values(&mut s, Value::lclosure(cl), &[Value::int(9)], LUA_MULTRET).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].as_int(), Some(9));
    collect_all(&mut s);
}

// ---------------------------------------------------------------------------------------------
// continuations, hooks, emergency collections
// ---------------------------------------------------------------------------------------------

fn k_cont(_st: &mut State, status: Status, ctx: isize) -> LuaResult<usize> {
    log(format!("k {status:?} {ctx}"));
    Ok(0)
}

fn n_outer_pcallk(st: &mut State) -> LuaResult<usize> {
    let func = st.top();
    st.push(Value::native(n_sum));
    st.push(Value::int(20));
    st.push(Value::int(22));
    let status = st.pcall_k(func, 1, 0, 7, Some(k_cont))?;
    assert_eq!(status, Status::Ok);
    let ci = st.ci();
    assert_eq!(
        ci.r().callstatus & CIST_YPCALL,
        0,
        "flag cleared after the call"
    );
    log(format!(
        "k_set={} ctx={} yieldable={} result={:?}",
        ci.r().k.is_some(),
        ci.r().ctx,
        st.yieldable(),
        st.get(func).as_int()
    ));
    Ok(1)
}

#[test]
fn pcall_k_with_and_without_yieldable_thread() {
    let mut s = new_state();
    take_log();
    // main thread: not yieldable -> plain protected call
    let r = call_values(&mut s, Value::native(n_outer_pcallk), &[], 1).unwrap();
    assert_eq!(r[0].as_int(), Some(42));
    assert_eq!(
        take_log(),
        vec!["k_set=false ctx=0 yieldable=false result=Some(42)"]
    );
    // coroutine thread: yieldable -> continuation recorded in the frame
    let th = s.new_thread().unwrap();
    // SAFETY: single user.
    let mut t = unsafe { State::from_gc(th) };
    assert!(t.yieldable());
    let r = call_values(&mut t, Value::native(n_outer_pcallk), &[], 1).unwrap();
    assert_eq!(r[0].as_int(), Some(42));
    assert_eq!(
        take_log(),
        vec!["k_set=true ctx=7 yieldable=true result=Some(42)"]
    );
    assert_eq!(t.errfunc(), 0);
    // call_k likewise
    fn n_callk(st: &mut State) -> LuaResult<usize> {
        let func = st.top();
        st.push(Value::native(n_sum));
        st.push(Value::int(1));
        st.call_k(func, LUA_MULTRET, 3, Some(k_cont))?;
        assert!(st.ci().r().k.is_some());
        Ok(st.top() - func)
    }
    let r = call_values(&mut t, Value::native(n_callk), &[], LUA_MULTRET).unwrap();
    assert_eq!(r[0].as_int(), Some(1));
    s.pop();
}

fn test_hook(_st: &mut State, event: i32, _line: i32) -> LuaResult<()> {
    log(format!("hook {event}"));
    Ok(())
}

#[test]
fn call_and_return_hooks_for_native_functions() {
    let mut s = new_state();
    take_log();
    s.set_hook(Some(test_hook), LUA_MASKCALL | LUA_MASKRET, 0);
    let top = s.top();
    let r = call_values(&mut s, Value::native(n_sum), &[Value::int(1)], 1).unwrap();
    assert_eq!(r[0].as_int(), Some(1));
    assert_eq!(take_log(), vec!["hook 0", "hook 1"]);
    assert!(s.allow_hook());
    assert_eq!(s.top(), top);
    // no hook inside a hook / when cleared
    s.set_hook(None, 0, 0);
    call_values(&mut s, Value::native(n_sum), &[], 0).unwrap();
    assert!(take_log().is_empty());
}

#[test]
fn emergency_collection_recovers_from_allocation_failure() {
    let mut s = new_state();
    collect_all(&mut s);
    let base = s.heap().total_bytes();
    s.heap().mem_mut().set_limit(base + 30_000);
    let cycles = s.g().heap.gc_cycles();
    // garbage is never collected by safepoints here; only emergency collections can save us
    for i in 0..3000 {
        let t = s
            .new_table()
            .unwrap_or_else(|e| panic!("alloc {i} failed: {e:?}"));
        let _ = t;
        let _ = s
            .new_str(format!("garbage string number {i} .................").as_bytes())
            .unwrap();
    }
    assert!(s.g().heap.gc_cycles() > cycles);
    // data on the stack survives emergency collections
    let keep = s.new_table().unwrap();
    s.push_checked(Value::table(keep)).unwrap();
    let ks = s.new_str_value(b"kept").unwrap();
    s.raw_set_int(keep, 1, &ks).unwrap();
    for i in 0..3000 {
        let _ = s
            .new_table()
            .unwrap_or_else(|e| panic!("alloc {i} failed: {e:?}"));
    }
    assert_eq!(keep.get_int(1).as_string().unwrap().as_bytes(), b"kept");
    s.heap().mem_mut().set_limit(usize::MAX);
    s.set_top(StkId(1));
    collect_all(&mut s);
}

#[test]
fn check_min_stack_and_create_thread_failure() {
    let mut s = new_state();
    assert!(s.check_min_stack());
    assert!(s.ci().next().is_some() && s.ci().next().and_then(|n| n.next()).is_some());
    // a failing thread creation leaves the state consistent
    let lim = s.heap().total_bytes() + 100;
    s.heap().mem_mut().set_limit(lim);
    assert!(s.new_thread().is_err());
    s.heap().mem_mut().set_limit(usize::MAX);
    assert_eq!(s.top(), StkId(1));
    collect_all(&mut s);
}

// JIT hooks (feature `jit`): the hot counters fire the `jit_hot` stub, nothing else changes.
#[cfg(all(feature = "jit", not(feature = "jit_no_loop")))]
#[test]
fn jit_hot_counters_fire_at_back_edges_and_frame_starts() {
    use crate::jit_hooks::hot_fired;
    let mut s = new_state();
    // for i = 1, 5000 do end
    let pd = pdata(
        vec![
            insi(OpCode::Loadi, 0, 1),
            insi(OpCode::Loadi, 1, 5000),
            insi(OpCode::Loadi, 2, 1),
            Instruction::abx(OpCode::Forprep, 0, 0).0,
            Instruction::abx(OpCode::Forloop, 0, 1).0,
            ins(OpCode::Return, 0, 1, 0),
        ],
        0,
        4,
    );
    let f = Value::lclosure(make_closure(&mut s, &pd));
    let old = crate::jit_hooks::set_reset_value(crate::jit_hooks::HOT_THRESHOLD);
    let before = hot_fired();
    call_values(&mut s, f, &[], 0).unwrap();
    crate::jit_hooks::set_reset_value(old);
    // 5000 back-edges: counter A fires 5 times (threshold 1000); B (global tick) at least 4
    assert!(hot_fired() - before >= 4, "loop: {}", hot_fired() - before);
}

// The hit case of the E1 check: a proto with a non-null `jit` reaches `jit_try_enter` (which
// declines), and execution is unaffected. Placement (c): once per frame start; (d): per call.
#[cfg(feature = "jit")]
#[test]
fn jit_entry_check_hit_case_declines() {
    use crate::jit_hooks::enter_calls;
    let mut s = new_state();
    let pd = pdata(
        vec![insi(OpCode::Loadi, 0, 42), ins(OpCode::Return, 0, 2, 0)],
        0,
        2,
    );
    let cl = make_closure(&mut s, &pd);
    // SAFETY: test-only; `jit` is never dereferenced by the prototype hooks.
    unsafe {
        (*cl.proto().as_ptr()).jit.set(8 as *mut u8);
        cl.proto().hot.set(0); // placement (e): compiled protos keep their counter <= 0
    }
    let f = Value::lclosure(cl);
    let before = enter_calls();
    let r = call_values(&mut s, f, &[], 1).unwrap();
    assert_eq!(r[0].as_int(), Some(42));
    assert!(enter_calls() > before);
}
