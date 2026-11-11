//! Integration tests of the embedding API (`rustynail::Lua` / `dyn Stack`), both Lua versions.
use rustynail::{ErrorKind, GcMode, LoadMode, Lua, Resume, StdLib, ThreadStatus, Type, MULTRET};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn lua() -> Lua {
    Lua::new().expect("state")
}

/// Evaluate an expression, return its `tostring`.
fn eval(l: &mut Lua, expr: &str) -> String {
    let n = l.do_string(&format!("return tostring({expr})")).unwrap();
    assert_eq!(n, 1);
    let s = l.to_string_lossy(-1).unwrap();
    l.pop(1).unwrap();
    s
}

#[test]
fn push_and_read_values() {
    let mut l = lua();
    assert_eq!(l.top(), 0);
    l.push_nil().unwrap();
    l.push_boolean(true).unwrap();
    l.push_integer(-7).unwrap();
    l.push_number(2.5).unwrap();
    l.push_bytes(b"a\0b").unwrap();
    l.push_str("10").unwrap();
    l.new_table().unwrap();
    assert_eq!(l.top(), 7);
    let types: Vec<_> = (1..=7).map(|i| l.type_of(i).unwrap()).collect();
    assert_eq!(
        types,
        [
            Type::Nil,
            Type::Boolean,
            Type::Number,
            Type::Number,
            Type::String,
            Type::String,
            Type::Table
        ]
    );
    assert!(l.is_nil(1));
    assert!(l.to_boolean(2) && !l.to_boolean(1));
    assert_eq!(l.to_integer(3), Some(-7));
    assert!(l.is_integer(3) && !l.is_integer(4));
    assert_eq!(l.to_number(4), Some(2.5));
    assert_eq!(l.to_integer(4), None); // not an exact integer
    assert_eq!(l.to_bytes(5).unwrap(), b"a\0b");
    assert_eq!(l.raw_len(5), 3);
    assert_eq!(l.to_integer(6), Some(10)); // string -> number conversion
    assert!(l.is_number(6) && l.is_string(6) && l.is_string(3));
    assert_eq!(l.to_bytes(3).unwrap(), b"-7"); // number -> string, slot unchanged
    assert!(l.is_integer(3));
    assert_eq!(l.type_name(7), "table");
    // invalid indices are "no value"
    assert_eq!(l.type_of(0), None);
    assert_eq!(l.type_of(8), None);
    assert_eq!(l.type_of(-8), None);
    assert_eq!(l.type_name(100), "no value");
    assert!(l.is_none_or_nil(100));
    assert_eq!(l.to_integer(100), None);
    assert_eq!(l.abs_index(-1), 7);
    assert_eq!(l.push_value(100).unwrap_err().kind(), ErrorKind::Api);
    l.set_top(0).unwrap();
    assert_eq!(l.top(), 0);
}

#[test]
fn stack_manipulation() {
    let mut l = lua();
    for i in 1..=5 {
        l.push_integer(i).unwrap();
    }
    let dump = |l: &Lua| -> Vec<i64> { (1..=l.top()).map(|i| l.to_integer(i).unwrap()).collect() };
    l.rotate(2, 1).unwrap();
    assert_eq!(dump(&l), [1, 5, 2, 3, 4]);
    l.rotate(2, -1).unwrap();
    assert_eq!(dump(&l), [1, 2, 3, 4, 5]);
    l.insert(1).unwrap();
    assert_eq!(dump(&l), [5, 1, 2, 3, 4]);
    l.remove(1).unwrap();
    assert_eq!(dump(&l), [1, 2, 3, 4]);
    l.push_integer(9).unwrap();
    l.replace(2).unwrap();
    assert_eq!(dump(&l), [1, 9, 3, 4]);
    l.copy(1, -1).unwrap();
    assert_eq!(dump(&l), [1, 9, 3, 1]);
    l.push_value(-3).unwrap();
    assert_eq!(dump(&l), [1, 9, 3, 1, 9]);
    l.set_top(7).unwrap();
    assert!(l.is_nil(6) && l.is_nil(7));
    l.set_top(2).unwrap();
    assert_eq!(dump(&l), [1, 9]);
    assert_eq!(l.pop(3).unwrap_err().kind(), ErrorKind::Api);
    // pushing far beyond LUA_MINSTACK grows the stack
    for i in 0..100_000 {
        l.push_integer(i).unwrap();
    }
    assert_eq!(l.top(), 100_002);
    assert_eq!(l.to_integer(-1), Some(99_999));
    l.set_top(0).unwrap();
}

#[test]
fn globals_and_do_string() {
    let mut l = lua();
    l.push_integer(41).unwrap();
    l.set_global("x").unwrap();
    assert_eq!(l.do_string("x = x + 1; return x, 'a', nil").unwrap(), 3);
    assert_eq!(l.to_integer(1), Some(42));
    assert!(l.is_nil(3));
    l.set_top(0).unwrap();
    assert_eq!(l.get_global("x").unwrap(), Type::Number);
    assert_eq!(l.get_global("nope").unwrap(), Type::Nil);
    l.set_top(0).unwrap();
    assert_eq!(eval(&mut l, "_VERSION"), rustynail::VERSION);
    assert_eq!(
        rustynail::VERSION_NUM,
        if cfg!(rustynail_lua54) { 504 } else { 505 }
    );
}

#[test]
fn tables_fields_and_metamethods() {
    let mut l = lua();
    l.create_table(4, 4).unwrap();
    for i in 1..=3 {
        l.push_integer(i * 10).unwrap();
        l.set_i(1, i).unwrap();
    }
    l.push_str("v").unwrap();
    l.set_field(1, "k").unwrap();
    l.push_str("key").unwrap();
    l.push_boolean(true).unwrap();
    l.set_table(1).unwrap();
    assert_eq!(l.get_i(1, 2).unwrap(), Type::Number);
    assert_eq!(l.to_integer(-1), Some(20));
    l.pop(1).unwrap();
    assert_eq!(l.get_field(1, "k").unwrap(), Type::String);
    l.pop(1).unwrap();
    l.push_str("key").unwrap();
    assert_eq!(l.get_table(1).unwrap(), Type::Boolean);
    l.pop(1).unwrap();
    assert_eq!(l.raw_get_i(1, 3).unwrap(), Type::Number);
    l.pop(1).unwrap();
    assert_eq!(l.raw_len(1), 3);
    l.len(1).unwrap();
    assert_eq!(l.to_integer(-1), Some(3));
    l.pop(1).unwrap();
    // traversal
    l.push_nil().unwrap();
    let mut n = 0;
    while l.next(1).unwrap() {
        n += 1;
        l.pop(1).unwrap();
    }
    assert_eq!(n, 5);
    assert_eq!(l.top(), 1);
    // metatable with __index / __newindex / __len written in Lua
    l.do_string(
        "return { __index = function(t, k) return k .. '!' end,
                  __newindex = function(t, k, v) rawset(t, k, v * 2) end,
                  __len = function() return 99 end }",
    )
    .unwrap();
    l.set_metatable(1).unwrap();
    assert_eq!(l.get_field(1, "zz").unwrap(), Type::String);
    assert_eq!(l.to_bytes(-1).unwrap(), b"zz!");
    l.pop(1).unwrap();
    l.push_integer(5).unwrap();
    l.set_field(1, "new").unwrap();
    l.push_str("new").unwrap();
    l.raw_get(1).unwrap();
    assert_eq!(l.to_integer(-1), Some(10));
    l.pop(1).unwrap();
    assert_eq!(l.len_integer(1).unwrap(), 99);
    assert!(l.get_metatable(1).unwrap());
    l.pop(1).unwrap();
    // raw access on a non-table is an API error, indexing nil a Lua error
    l.push_integer(1).unwrap();
    assert_eq!(l.raw_get_i(-1, 1).unwrap_err().kind(), ErrorKind::Api);
    l.push_nil().unwrap();
    let e = l.get_field(-1, "x").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Runtime);
    assert!(
        e.message_lossy().contains("attempt to index a nil value"),
        "{e}"
    );
    // raw_set with a nil key fails and pops key and value
    let top = l.top();
    l.push_nil().unwrap();
    l.push_integer(1).unwrap();
    assert!(l
        .raw_set(1)
        .unwrap_err()
        .message_lossy()
        .contains("index is nil"));
    assert_eq!(l.top(), top);
}

#[test]
fn comparisons_concat_tostring() {
    let mut l = lua();
    l.push_integer(1).unwrap();
    l.push_number(1.0).unwrap();
    l.push_integer(2).unwrap();
    assert!(l.equal(1, 2).unwrap());
    assert!(l.raw_equal(1, 2));
    assert!(l.less_than(1, 3).unwrap());
    assert!(l.less_equal(1, 2).unwrap());
    assert!(!l.raw_equal(1, 100));
    l.push_str("x").unwrap();
    let e = l.less_than(1, 4).unwrap_err();
    assert!(e.message_lossy().contains("attempt to compare"), "{e}");
    l.set_top(0).unwrap();
    l.push_str("a").unwrap();
    l.push_integer(1).unwrap();
    l.push_str("b").unwrap();
    l.concat(3).unwrap();
    assert_eq!(l.to_bytes(-1).unwrap(), b"a1b");
    l.concat(0).unwrap();
    assert_eq!(l.to_bytes(-1).unwrap(), b"");
    l.new_table().unwrap();
    l.concat(2).unwrap_err();
    assert_eq!(l.top(), 1); // operands popped on error
    l.do_string("return setmetatable({}, {__tostring = function() return 'OBJ' end})")
        .unwrap();
    assert_eq!(l.tostring(-1).unwrap(), b"OBJ");
    l.push_number(0.1).unwrap();
    assert_eq!(l.tostring(-1).unwrap(), b"0.1");
    assert_eq!(l.top(), 3);
}

#[test]
fn syntax_and_runtime_errors() {
    let mut l = lua();
    let e = l.do_string("x = = 1").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Syntax);
    assert!(e.message_lossy().contains("unexpected symbol"), "{e}");
    assert_eq!(l.top(), 0);
    let e = l.do_string("error('boom')").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Runtime);
    assert_eq!(e.message(), b"[string \"error('boom')\"]:1: boom");
    assert_eq!(e.to_string(), "[string \"error('boom')\"]:1: boom");
    let e = l.do_string("error({code = 7})").unwrap_err();
    assert_eq!(e.message(), b"(error object is a table value)");
    assert_eq!(l.top(), 0);
    // pcall keeps the error object on the stack
    l.load_string("error({code = 7})").unwrap();
    assert!(l.pcall(0, 0, 0).is_err());
    assert_eq!(l.top(), 1);
    assert_eq!(l.get_field(1, "code").unwrap(), Type::Number);
    assert_eq!(l.to_integer(-1), Some(7));
    l.set_top(0).unwrap();
    // load with a chunk name and mode
    l.load(b"return 1 +", "=mychunk", LoadMode::Text)
        .unwrap_err();
    let e = l
        .load(b"return 1 +", "=mychunk", LoadMode::Text)
        .unwrap_err();
    assert!(e.message_lossy().starts_with("mychunk:1:"), "{e}");
    let e = l.load(b"return 1", "=c", LoadMode::Binary).unwrap_err();
    assert!(
        e.message_lossy().contains("attempt to load a text chunk"),
        "{e}"
    );
    // message handler with traceback
    l.push_traceback_handler().unwrap();
    l.load_string("local function f() error('deep') end f()")
        .unwrap();
    let e = l.pcall(0, 0, 1).unwrap_err();
    assert!(e.message_lossy().contains("stack traceback:"), "{e}");
    assert!(e.message_lossy().contains("deep"), "{e}");
    l.set_top(0).unwrap();
}

#[test]
fn rust_functions() {
    let mut l = lua();
    l.push_function(|s| {
        let n = s.top();
        let mut sum = 0.0;
        for i in 1..=n {
            sum += s
                .to_number(i)
                .ok_or_else(|| rustynail::Error::runtime("number expected"))?;
        }
        s.push_number(sum)?;
        s.push_integer(i64::from(n))?;
        Ok(2)
    })
    .unwrap();
    l.set_global("sum").unwrap();
    assert_eq!(eval(&mut l, "select('#', sum(1, 2))"), "2");
    assert_eq!(eval(&mut l, "sum(1, 2, 3.5)"), "6.5");
    // a Rust error is a Lua error that Lua code can catch
    assert_eq!(
        eval(&mut l, "select(2, pcall(sum, 1, {}))"),
        "number expected"
    );
    // errors propagate out of nested Rust <-> Lua calls with the original object
    l.push_function(|s| {
        // call the Lua function given as argument 1; `?` re-raises its error object unchanged
        s.push_value(1)?;
        s.call(0, 0)?;
        Ok(0)
    })
    .unwrap();
    l.set_global("invoke").unwrap();
    assert_eq!(
        eval(
            &mut l,
            "select(2, pcall(invoke, function() error({tag = 'T'}) end)).tag"
        ),
        "T"
    );
    // Stack::error raises an arbitrary value
    l.push_function(|s| {
        s.new_table()?;
        s.push_integer(5)?;
        s.set_field(-2, "n")?;
        Err(s.error())
    })
    .unwrap();
    l.set_global("raise_table").unwrap();
    assert_eq!(eval(&mut l, "select(2, pcall(raise_table)).n"), "5");
    // a panic becomes a Lua error
    l.push_function(|_| panic!("oops")).unwrap();
    l.set_global("panics").unwrap();
    let msg = eval(&mut l, "select(2, pcall(panics))");
    assert!(msg.contains("Rust function panicked: oops"), "{msg}");
    // returning more results than pushed is caught
    l.push_function(|_| Ok(3)).unwrap();
    l.set_global("bad").unwrap();
    assert!(eval(&mut l, "select(2, pcall(bad))").contains("more results"));
    // the state is still fine
    assert_eq!(eval(&mut l, "1 + 1"), "2");
    assert_eq!(l.top(), 0);
}

#[test]
fn closures_capture_and_upvalues() {
    let mut l = lua();
    // Rust captures
    let calls = Rc::new(Cell::new(0));
    let c2 = calls.clone();
    l.push_function(move |_| {
        c2.set(c2.get() + 1);
        Ok(0)
    })
    .unwrap();
    l.set_global("tick").unwrap();
    l.do_string("for i = 1, 10 do tick() end").unwrap();
    assert_eq!(calls.get(), 10);
    // Lua upvalues (lua_pushcclosure)
    l.push_integer(100).unwrap();
    l.push_str("tag").unwrap();
    l.push_closure(
        |s| {
            s.push_upvalue(1)?;
            let v = s.to_integer(-1).unwrap() + 1;
            s.pop(1)?;
            s.push_integer(v)?;
            s.push_value(-1)?;
            s.replace_upvalue(1)?;
            s.push_upvalue(2)?;
            assert!(s.push_upvalue(3).is_err());
            Ok(2)
        },
        2,
    )
    .unwrap();
    assert_eq!(l.top(), 1);
    l.set_global("counter").unwrap();
    assert_eq!(eval(&mut l, "counter()"), "101");
    assert_eq!(eval(&mut l, "counter()"), "102");
    assert_eq!(eval(&mut l, "select(2, counter())"), "tag");
    // upvalues outside a Rust closure
    assert_eq!(l.push_upvalue(1).unwrap_err().kind(), ErrorKind::Api);
}

struct DropCount(Rc<Cell<u32>>);
impl Drop for DropCount {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn closures_are_dropped_by_the_collector_and_on_close() {
    let drops = Rc::new(Cell::new(0));
    let mut l = lua();
    for _ in 0..10 {
        let d = DropCount(drops.clone());
        l.push_function(move |_| {
            let _ = &d;
            Ok(0)
        })
        .unwrap();
        l.pop(1).unwrap();
    }
    l.gc_collect();
    l.gc_collect();
    assert_eq!(drops.get(), 10);
    let d = DropCount(drops.clone());
    l.push_function(move |_| {
        let _ = &d;
        Ok(0)
    })
    .unwrap();
    l.set_global("kept").unwrap();
    l.gc_collect();
    assert_eq!(drops.get(), 10);
    drop(l);
    assert_eq!(drops.get(), 11);
}

#[derive(Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

#[test]
fn userdata_with_methods() {
    let mut l = lua();
    // methods table on the type's metatable
    l.push_userdata_metatable::<Point>().unwrap();
    l.new_table().unwrap();
    l.push_function(|s| {
        let (x, y) = s.with_userdata(1, |p: &Point| (p.x, p.y))?;
        s.push_number((x * x + y * y).sqrt())?;
        Ok(1)
    })
    .unwrap();
    l.set_field(-2, "len").unwrap();
    l.push_function(|s| {
        let f = s.to_number(2).unwrap_or(1.0);
        s.with_userdata_mut(1, |p: &mut Point| {
            p.x *= f;
            p.y *= f;
        })?;
        Ok(0)
    })
    .unwrap();
    l.set_field(-2, "scale").unwrap();
    l.set_field(-2, "__index").unwrap();
    l.pop(1).unwrap();
    // constructor
    l.push_function(|s| {
        let p = Point {
            x: s.to_number(1).unwrap_or(0.0),
            y: s.to_number(2).unwrap_or(0.0),
        };
        s.push_userdata(p)?;
        Ok(1)
    })
    .unwrap();
    l.set_global("Point").unwrap();
    assert_eq!(eval(&mut l, "Point(3, 4):len()"), "5.0");
    l.do_string("p = Point(1, 2); p:scale(10)").unwrap();
    l.get_global("p").unwrap();
    assert!(l.is_userdata::<Point>(-1));
    assert!(!l.is_userdata::<String>(-1));
    assert_eq!(l.type_of(-1), Some(Type::Userdata));
    let p = l.with_userdata(-1, |p: &Point| (p.x, p.y)).unwrap();
    assert_eq!(p, (10.0, 20.0));
    let e = l.with_userdata(-1, |_: &String| ()).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Api);
    assert!(e.to_string().contains("does not hold"), "{e}");
    // tostring uses __name
    let s = l.tostring(-1).unwrap();
    assert!(
        String::from_utf8_lossy(&s).contains("Point"),
        "{}",
        String::from_utf8_lossy(&s)
    );
    // the metatable of a Rust userdata cannot be replaced
    l.new_table().unwrap();
    assert_eq!(l.set_metatable(-2).unwrap_err().kind(), ErrorKind::Api);
    l.set_top(0).unwrap();
    // wrong argument type from Lua
    let msg = eval(&mut l, "select(2, pcall(Point(1,1).len, {}))");
    assert!(msg.contains("userdata expected"), "{msg}");
    // calling a method of a finalized object (manual __gc) is an error, not a crash
    l.do_string(
        "local p = Point(1, 1); local mt = debug.getmetatable(p); mt.__gc(p)
         ok, err = pcall(p.len, p)",
    )
    .unwrap();
    assert_eq!(eval(&mut l, "ok"), "false");
    assert!(eval(&mut l, "err").contains("finalized"));
}

#[test]
fn userdata_are_dropped() {
    let drops = Rc::new(Cell::new(0));
    let mut l = lua();
    for _ in 0..5 {
        l.push_userdata(DropCount(drops.clone())).unwrap();
        l.pop(1).unwrap();
    }
    l.gc_collect();
    l.gc_collect();
    assert_eq!(drops.get(), 5);
    l.push_userdata(DropCount(drops.clone())).unwrap();
    l.set_global("u").unwrap();
    drop(l);
    assert_eq!(drops.get(), 6);
}

#[test]
fn registry_references_and_light_userdata() {
    let mut l = lua();
    l.push_str("stored").unwrap();
    let k = l.reference().unwrap();
    assert_eq!(l.top(), 0);
    l.gc_collect();
    l.push_reference(&k).unwrap();
    assert_eq!(l.to_bytes(-1).unwrap(), b"stored");
    l.pop(1).unwrap();
    l.unreference(k).unwrap();
    l.push_nil().unwrap();
    let nil_key = l.reference().unwrap();
    l.push_reference(&nil_key).unwrap();
    assert!(l.is_nil(-1));
    l.pop(1).unwrap();
    // functions survive in the registry
    l.do_string("return function(a) return a * 2 end").unwrap();
    let f = l.reference().unwrap();
    l.push_reference(&f).unwrap();
    l.push_integer(21).unwrap();
    l.call(1, 1).unwrap();
    assert_eq!(l.to_integer(-1), Some(42));
    l.pop(1).unwrap();
    // light userdata are compared by address
    let mut x = 5u8;
    let p = std::ptr::addr_of_mut!(x).cast::<std::ffi::c_void>();
    l.push_light_userdata(p).unwrap();
    l.push_light_userdata(p).unwrap();
    assert!(l.raw_equal(-1, -2));
    assert_eq!(l.to_light_userdata(-1), Some(p));
    assert_eq!(l.type_of(-1), Some(Type::LightUserdata));
    l.push_registry().unwrap();
    assert!(l.is_table(-1));
    l.push_globals().unwrap();
    l.get_field(-1, "print").unwrap();
    assert!(l.is_function(-1));
    assert!(l.push_thread().unwrap());
}

#[test]
fn call_and_pcall_results() {
    let mut l = lua();
    l.do_string("function three() return 1, 2, 3 end").unwrap();
    l.get_global("three").unwrap();
    l.call(0, MULTRET).unwrap();
    assert_eq!(l.top(), 3);
    l.set_top(0).unwrap();
    l.get_global("three").unwrap();
    l.call(0, 5).unwrap();
    assert_eq!(l.top(), 5);
    assert!(l.is_nil(5));
    l.set_top(0).unwrap();
    l.get_global("three").unwrap();
    l.pcall(0, 1, 0).unwrap();
    assert_eq!(l.top(), 1);
    l.set_top(0).unwrap();
    // not enough values
    assert_eq!(l.call(2, 0).unwrap_err().kind(), ErrorKind::Api);
    // calling a non-function
    l.push_integer(1).unwrap();
    let e = l.call(0, 0).unwrap_err();
    assert!(e.message_lossy().contains("attempt to call"), "{e}");
    assert_eq!(l.top(), 0);
}

#[test]
fn coroutines_from_rust() {
    let mut l = lua();
    l.do_string(
        "return function(a, b)
            local c = coroutine.yield(a + b)
            local d, e = coroutine.yield(c * 2)
            return d + e, 'end'
         end",
    )
    .unwrap();
    l.create_coroutine().unwrap();
    assert_eq!(l.type_of(1), Some(Type::Thread));
    assert_eq!(l.thread_status(1).unwrap(), ThreadStatus::Suspended);
    l.push_integer(1).unwrap();
    l.push_integer(2).unwrap();
    assert_eq!(l.resume(1, 2).unwrap(), Resume::Yield(1));
    assert_eq!(l.to_integer(-1), Some(3));
    l.pop(1).unwrap();
    l.push_integer(10).unwrap();
    assert_eq!(l.resume(1, 1).unwrap(), Resume::Yield(1));
    assert_eq!(l.to_integer(-1), Some(20));
    l.pop(1).unwrap();
    l.push_integer(4).unwrap();
    l.push_integer(5).unwrap();
    assert_eq!(l.resume(1, 2).unwrap(), Resume::Return(2));
    assert_eq!(l.to_integer(-2), Some(9));
    l.pop(2).unwrap();
    assert_eq!(l.thread_status(1).unwrap(), ThreadStatus::Dead);
    let e = l.resume(1, 0).unwrap_err();
    assert!(e.message_lossy().contains("dead coroutine"), "{e}");
    l.set_top(0).unwrap();
    // an error kills the coroutine; the error object is pushed
    l.do_string("return function() error({x = 1}) end").unwrap();
    l.create_coroutine().unwrap();
    assert!(l.resume(1, 0).is_err());
    assert_eq!(l.top(), 2);
    assert!(l.is_table(2));
    assert_eq!(l.thread_status(1).unwrap(), ThreadStatus::Dead);
    l.set_top(0).unwrap();
    // closing a suspended coroutine runs its to-be-closed variables
    l.do_string(
        "closed = false
         return function()
           local x <close> = setmetatable({}, {__close = function() closed = true end})
           coroutine.yield()
         end",
    )
    .unwrap();
    l.create_coroutine().unwrap();
    assert_eq!(l.resume(1, 0).unwrap(), Resume::Yield(0));
    l.close_thread(1).unwrap();
    assert_eq!(eval(&mut l, "closed"), "true");
    assert_eq!(l.thread_status(1).unwrap(), ThreadStatus::Dead);
    assert!(!l.is_yieldable());
}

#[test]
fn rust_functions_yield() {
    let mut l = lua();
    // a Rust function that yields its arguments doubled; resume values become its results
    l.push_function(|s| {
        let n = s.top();
        for i in 1..=n {
            let v = s.to_integer(i).unwrap_or(0);
            s.push_integer(v * 2)?;
        }
        s.yield_(n as usize)
    })
    .unwrap();
    l.set_global("ryield").unwrap();
    l.do_string(
        "co = coroutine.wrap(function(a)
            local r1, r2 = ryield(a, a + 1)
            return 'got', r1, r2
         end)",
    )
    .unwrap();
    assert_eq!(eval(&mut l, "table.concat({co(5)}, ',')"), "10,12");
    assert_eq!(eval(&mut l, "table.concat({co('x', 'y')}, ',')"), "got,x,y");
    // yielding outside a coroutine is a Lua error
    let msg = eval(&mut l, "select(2, pcall(ryield, 1))");
    assert!(msg.contains("outside a coroutine"), "{msg}");
    // a coroutine whose body is a Rust function, driven from Rust
    l.push_function(|s| {
        s.push_str("first")?;
        s.yield_(1)
    })
    .unwrap();
    l.create_coroutine().unwrap();
    assert_eq!(l.resume(-1, 0).unwrap(), Resume::Yield(1));
    assert_eq!(l.to_bytes(-1).unwrap(), b"first");
    l.pop(1).unwrap();
    l.push_str("back").unwrap();
    assert_eq!(l.resume(-2, 1).unwrap(), Resume::Return(1));
    assert_eq!(l.to_bytes(-1).unwrap(), b"back");
}

#[test]
fn garbage_collector_control() {
    let mut l = lua();
    let base = l.gc_count();
    l.do_string("t = {} for i = 1, 10000 do t[i] = {i} end")
        .unwrap();
    let grown = l.gc_count();
    assert!(grown > base + 100_000, "{base} {grown}");
    l.do_string("t = nil").unwrap();
    l.gc_collect();
    assert!(l.gc_count() < grown / 2);
    assert!(l.gc_is_running());
    l.gc_stop();
    assert!(!l.gc_is_running());
    l.gc_restart();
    assert!(l.gc_is_running());
    assert_eq!(l.set_gc_mode(GcMode::Generational), GcMode::Incremental);
    assert_eq!(l.set_gc_mode(GcMode::Incremental), GcMode::Generational);
    let mut finished = false;
    for _ in 0..1000 {
        if l.gc_step() {
            finished = true;
            break;
        }
    }
    assert!(finished);
    #[cfg(not(rustynail_lua54))]
    {
        use rustynail::GcParam;
        let old = l.gc_param(GcParam::Pause, Some(300));
        assert_eq!(l.gc_param(GcParam::Pause, None), 300);
        l.gc_param(GcParam::Pause, Some(old));
    }
    #[cfg(rustynail_lua54)]
    {
        assert_eq!(l.gc_generational(20, 100), GcMode::Incremental);
        assert_eq!(l.gc_incremental(200, 100, 13), GcMode::Generational);
    }
}

#[test]
fn memory_limit() {
    let mut l = Lua::builder().memory_limit(4 << 20).build().unwrap();
    let e = l
        .do_string("local t = {} for i = 1, 1e7 do t[i] = i end")
        .unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Memory);
    assert_eq!(e.message(), b"not enough memory");
    // the state still works and Lua code sees the memory error as an error value
    l.gc_collect();
    assert_eq!(
        eval(&mut l, "select(2, pcall(string.rep, 'x', 1 << 23))"),
        "not enough memory"
    );
    l.set_memory_limit(None);
    assert_eq!(eval(&mut l, "#string.rep('x', 1 << 23)"), "8388608");
    assert!(l.memory_used() > 0);
}

#[test]
fn instruction_limit_cannot_be_caught_by_scripts() {
    let mut l = Lua::builder().instruction_limit(100_000).build().unwrap();
    assert_eq!(l.instruction_limit(), Some(100_000));
    let e = l.do_string("while true do end").unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InstructionLimit);
    assert!(
        e.message_lossy().contains("instruction limit exceeded"),
        "{e}"
    );
    // pcall inside the script does not stop it
    let e = l
        .do_string("for i = 1, 100 do pcall(function() while true do end end) end return 'escaped'")
        .unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InstructionLimit);
    // nor does a Rust function in between
    l.push_function(|s| {
        s.push_value(1)?;
        let _ = s.call(0, 0); // swallow the error
        Ok(0)
    })
    .unwrap();
    l.set_global("swallow").unwrap();
    let e = l
        .do_string("swallow(function() while true do end end) return 'escaped'")
        .unwrap_err();
    assert_eq!(e.kind(), ErrorKind::InstructionLimit);
    // short scripts still run, and the limit can be lifted
    l.set_instruction_limit(Some(1_000_000));
    assert_eq!(eval(&mut l, "1 + 2"), "3");
    l.set_instruction_limit(None);
    assert_eq!(l.instruction_limit(), None);
    l.do_string("for i = 1, 3e6 do end").unwrap();
    assert_eq!(l.top(), 0);
}

#[test]
fn sandbox() {
    let mut l = Lua::builder()
        .sandbox()
        .memory_limit(8 << 20)
        .instruction_limit(1_000_000)
        .build()
        .unwrap();
    for g in [
        "io", "os", "package", "debug", "require", "dofile", "loadfile",
    ] {
        assert_eq!(eval(&mut l, g), "nil", "{g}");
    }
    for g in [
        "string",
        "table",
        "math",
        "utf8",
        "coroutine",
        "print",
        "load",
    ] {
        assert_ne!(eval(&mut l, g), "nil", "{g}");
    }
    // binary chunks are refused even when asked for
    let msg = eval(
        &mut l,
        "select(2, load(string.dump(function() return 1 end), 'x', 'b'))",
    );
    assert!(msg.contains("attempt to load a binary chunk"), "{msg}");
    assert_eq!(eval(&mut l, "load('return 7')()"), "7");
    // the host cannot load binary chunks either
    let mut full = lua();
    full.do_string("return string.dump(function() return 1 end)")
        .unwrap();
    let chunk = full.to_bytes(-1).unwrap();
    assert!(full.load(&chunk, "=bin", LoadMode::Binary).is_ok());
    let e = l.load(&chunk, "=bin", LoadMode::TextOrBinary).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Syntax);
    assert_eq!(
        Lua::with_libs(StdLib::NONE)
            .unwrap()
            .get_global("print")
            .unwrap(),
        Type::Nil
    );
    let mut b = Lua::with_libs(StdLib::BASE | StdLib::MATH).unwrap();
    assert_eq!(eval(&mut b, "math.max(1, 2)"), "2");
    assert_eq!(eval(&mut b, "string"), "nil");
    assert!(StdLib::ALL.contains(StdLib::SAFE));
    assert!(!StdLib::SAFE.contains(StdLib::IO));
}

#[test]
fn luajit_extensions() {
    // not part of ALL / SAFE: opt-in
    let mut l = Lua::new().unwrap();
    assert_eq!(eval(&mut l, "type(table.new) .. type(jit)"), "nilnil");
    let mut e = Lua::with_libs(StdLib::ALL | StdLib::EXT).unwrap();
    assert_eq!(eval(&mut e, "type(table.new) .. type(table.clear) .. type(jit)"), "functionfunctionnil");
    assert_eq!(eval(&mut e, "#require('string.buffer').encode({1, 2, x = 3}) > 0 and 'ok'"), "ok");
    assert_eq!(eval(&mut e, "type(string.buffer)"), "nil");
    let mut j = Lua::with_libs(StdLib::ALL | StdLib::EXT | StdLib::JIT).unwrap();
    assert_eq!(eval(&mut j, "type(jit.status()) .. type(jit.version)"), "booleanstring");
    assert_eq!(eval(&mut j, "jit.off() or jit.status() == false"), "true");
    assert_eq!(eval(&mut j, "jit.on() or true"), "true");
    // a script cannot enable a compiler its host left off; `jit.on()` returns to the mode the host chose
    #[cfg(feature = "jit")]
    {
        assert_eq!(eval(&mut j, "jit.on() or jit.status()"), "false");
        if j.set_jit_mode(rustynail::JitMode::On) {
            assert_eq!(eval(&mut j, "jit.status()"), "true");
            assert_eq!(eval(&mut j, "jit.off() or jit.status()"), "false");
            assert_eq!(eval(&mut j, "jit.on() or jit.status()"), "true");
        }
        j.set_jit_mode(rustynail::JitMode::Off);
        assert_eq!(eval(&mut j, "jit.on() or jit.status()"), "false");
    }
    // the sandbox preset stays free of the jit table; extensions can be added explicitly
    let mut s = Lua::builder().sandbox().build().unwrap();
    assert_eq!(eval(&mut s, "type(jit) .. type(table.new)"), "nilnil");
    s.open_libs(StdLib::EXT).unwrap();
    assert_eq!(eval(&mut s, "type(table.new) .. type(table.clear)"), "functionfunction");
    let t = eval(&mut s, "(function() local t = table.new(8, 8) t.a = 1 t[1] = 2 table.clear(t) return next(t) == nil end)()");
    assert_eq!(t, "true");
}

#[test]
fn files() {
    let dir = std::env::temp_dir().join(format!(
        "rustynail-api-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let f = dir.join("script.lua");
    std::fs::write(&f, "#!/usr/bin/env lua\nlocal a = ... return 'ran', a\n").unwrap();
    let mut l = lua();
    assert_eq!(l.do_file(&f).unwrap(), 2);
    assert_eq!(l.to_bytes(1).unwrap(), b"ran");
    l.set_top(0).unwrap();
    l.load_file(&f, LoadMode::Text).unwrap();
    l.push_integer(3).unwrap();
    l.call(1, 1).unwrap();
    l.set_top(0).unwrap();
    let e = l.do_file(dir.join("missing.lua")).unwrap_err();
    assert_eq!(e.kind(), ErrorKind::File);
    assert!(e.message_lossy().starts_with("cannot open"), "{e}");
    std::fs::write(&f, "return +").unwrap();
    assert_eq!(l.do_file(&f).unwrap_err().kind(), ErrorKind::Syntax);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn deep_recursion_through_rust_is_an_error() {
    // Lua -> Rust -> Lua -> ... until the C-stack limit; run on a big stack like an embedder should
    let r = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(|| {
            let mut l = lua();
            l.push_function(|s| {
                s.get_global("f")?;
                s.push_value(1)?;
                s.call(1, 1)?;
                Ok(1)
            })
            .unwrap();
            l.set_global("viarust").unwrap();
            l.do_string("function f(x) return viarust(x) end").unwrap();
            let e = l.do_string("return f(1)").unwrap_err();
            (e.kind(), e.message_lossy())
        })
        .unwrap()
        .join()
        .unwrap();
    assert_eq!(r.0, ErrorKind::Runtime);
    assert!(r.1.contains("stack overflow"), "{}", r.1);
}

#[test]
fn finalizers_can_call_rust() {
    let log = Rc::new(RefCell::new(Vec::new()));
    let mut l = lua();
    let lg = log.clone();
    l.push_function(move |s| {
        lg.borrow_mut()
            .push(s.to_string_lossy(1).unwrap_or_default());
        Ok(0)
    })
    .unwrap();
    l.set_global("note").unwrap();
    l.do_string(
        "setmetatable({}, {__gc = function() note('collected') end})
         keep = setmetatable({}, {__gc = function() note('at close') end})",
    )
    .unwrap();
    l.gc_collect();
    assert_eq!(*log.borrow(), ["collected"]);
    drop(l);
    assert_eq!(*log.borrow(), ["collected", "at close"]);
}

#[test]
fn several_states_are_independent() {
    let mut a = lua();
    let mut b = lua();
    a.push_integer(1).unwrap();
    a.set_global("v").unwrap();
    b.push_integer(2).unwrap();
    b.set_global("v").unwrap();
    let ca = Rc::new(Cell::new(0));
    let c = ca.clone();
    a.push_function(move |_| {
        c.set(c.get() + 1);
        Ok(0)
    })
    .unwrap();
    a.set_global("f").unwrap();
    assert_eq!(eval(&mut a, "v"), "1");
    assert_eq!(eval(&mut b, "v"), "2");
    assert_eq!(eval(&mut b, "f"), "nil");
    a.do_string("f()").unwrap();
    drop(a);
    assert_eq!(ca.get(), 1);
    assert_eq!(eval(&mut b, "v + 1"), "3");
}

#[test]
fn errors_describe_themselves() {
    let e = rustynail::Error::runtime("custom");
    assert_eq!(e.kind(), ErrorKind::Runtime);
    assert_eq!(format!("{e}"), "custom");
    assert!(format!("{e:?}").contains("Runtime"));
    let boxed: Box<dyn std::error::Error> = Box::new(e);
    assert_eq!(boxed.to_string(), "custom");
    // a memory error raised by Rust is a memory error in Lua
    let mut l = lua();
    l.push_function(|_| Err(rustynail::Error::new(ErrorKind::Memory, "x")))
        .unwrap();
    l.set_global("oom").unwrap();
    assert_eq!(eval(&mut l, "select(2, pcall(oom))"), "not enough memory");
}
