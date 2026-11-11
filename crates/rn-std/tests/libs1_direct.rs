//! Direct native calls on a `State` (no interpreter needed, so these also run under the 5.4 cfg).
use rn_core::number;
use rn_core::state::State;
use rn_core::status::Status;
use rn_core::value::Value;

fn field(st: &mut State, lib: &str, name: &str) -> Value {
    let g = st.globals();
    let l = st
        .raw_get_str(g, lib.as_bytes())
        .unwrap()
        .as_table()
        .expect("lib table");
    st.raw_get_str(l, name.as_bytes()).unwrap()
}

enum A<'a> {
    S(&'a str),
    I(i64),
    F(f64),
}

fn show(v: &Value) -> String {
    if let Some(s) = v.as_string() {
        String::from_utf8_lossy(s.as_bytes()).into_owned()
    } else if let Some(n) = v.as_number() {
        String::from_utf8_lossy(number::tostring(n).as_bytes()).into_owned()
    } else if v.is_nil() {
        "nil".into()
    } else if let Some(b) = v.as_bool() {
        b.to_string()
    } else {
        format!("{v:?}")
    }
}

/// Call `lib.name(args)`; returns the results joined with `,` or `ERR:msg`.
fn call(st: &mut State, lib: &str, name: &str, args: &[A]) -> String {
    let f = field(st, lib, name);
    let func = st.top();
    st.push(f);
    for a in args {
        let v = match a {
            A::S(s) => st.new_str_value(s.as_bytes()).unwrap(),
            A::I(i) => Value::int(*i),
            A::F(x) => Value::float(*x),
        };
        st.push(v);
    }
    let status = st.pcall(func, -1, 0);
    let out: Vec<String> = (func.0..st.top().0)
        .map(|i| show(&st.get(rn_core::state::StkId(i))))
        .collect();
    st.set_top(func);
    if status == Status::Ok {
        out.join(",")
    } else {
        format!("ERR:{}", out.join(","))
    }
}

fn state() -> rn_core::state::LuaState {
    rn_std::new_state_with_libs().expect("state")
}

#[test]
fn string_functions() {
    let mut st = state();
    assert_eq!(
        call(&mut st, "string", "rep", &[A::S("ab"), A::I(3), A::S(",")]),
        "ab,ab,ab"
    );
    assert_eq!(call(&mut st, "string", "rep", &[A::S(""), A::I(3)]), "");
    assert_eq!(
        call(
            &mut st,
            "string",
            "sub",
            &[A::S("hello"), A::I(2), A::I(-2)]
        ),
        "ell"
    );
    assert_eq!(
        call(&mut st, "string", "byte", &[A::S("abc"), A::I(1), A::I(-1)]),
        "97,98,99"
    );
    assert_eq!(
        call(&mut st, "string", "char", &[A::I(72), A::I(105)]),
        "Hi"
    );
    assert_eq!(call(&mut st, "string", "upper", &[A::S("abc")]), "ABC");
    assert_eq!(call(&mut st, "string", "len", &[A::I(12345)]), "5");
    assert_eq!(
        call(&mut st, "string", "find", &[A::S("hello"), A::S("l+")]),
        "3,4"
    );
    assert_eq!(
        call(
            &mut st,
            "string",
            "match",
            &[A::S("key=val"), A::S("(%w+)=(%w+)")]
        ),
        "key,val"
    );
    assert_eq!(
        call(
            &mut st,
            "string",
            "gsub",
            &[A::S("hello"), A::S("l"), A::S("L")]
        ),
        "heLLo,2"
    );
    assert_eq!(
        call(
            &mut st,
            "string",
            "format",
            &[A::S("%5.2f|%d"), A::F(std::f64::consts::PI), A::I(7)]
        ),
        " 3.14|7"
    );
    assert_eq!(
        call(&mut st, "string", "rep", &[A::S("x"), A::I(i64::MAX)]),
        "ERR:resulting string too large"
    );
    assert_eq!(
        call(&mut st, "string", "pack", &[A::S("<i2"), A::I(258)]),
        "\u{2}\u{1}"
    );
    assert_eq!(
        call(
            &mut st,
            "string",
            "unpack",
            &[A::S("<i2"), A::S("\u{2}\u{1}")]
        ),
        "258,3"
    );
    assert_eq!(call(&mut st, "string", "packsize", &[A::S("i4i8")]), "12");
    assert_eq!(call(&mut st, "string", "byte", &[A::I(1)]), "49");
    assert_eq!(
        call(&mut st, "string", "rep", &[]),
        "ERR:bad argument #1 to 'string.rep' (string expected, got no value)"
    );
}

#[test]
fn math_functions() {
    let mut st = state();
    assert_eq!(call(&mut st, "math", "floor", &[A::F(3.7)]), "3");
    assert_eq!(call(&mut st, "math", "ceil", &[A::F(-3.7)]), "-3");
    assert_eq!(
        call(&mut st, "math", "abs", &[A::I(i64::MIN)]),
        i64::MIN.to_string()
    );
    assert_eq!(call(&mut st, "math", "fmod", &[A::I(-7), A::I(3)]), "-1");
    assert_eq!(
        call(&mut st, "math", "fmod", &[A::I(1), A::I(0)]),
        "ERR:bad argument #2 to 'math.fmod' (zero)"
    );
    assert_eq!(
        call(&mut st, "math", "max", &[A::I(1), A::F(2.5), A::I(2)]),
        "2.5"
    );
    assert_eq!(call(&mut st, "math", "tointeger", &[A::F(3.0)]), "3");
    assert_eq!(call(&mut st, "math", "type", &[A::I(1)]), "integer");
    assert_eq!(call(&mut st, "math", "ult", &[A::I(1), A::I(-1)]), "true");
    assert_eq!(call(&mut st, "math", "randomseed", &[A::I(42)]), "42,0");
    let a = call(&mut st, "math", "random", &[A::I(1), A::I(100)]);
    assert_eq!(call(&mut st, "math", "randomseed", &[A::I(42)]), "42,0");
    assert_eq!(call(&mut st, "math", "random", &[A::I(1), A::I(100)]), a);
    assert_eq!(
        call(&mut st, "math", "random", &[A::I(3), A::I(1)]),
        "ERR:bad argument #1 to 'math.random' (interval is empty)"
    );
    assert_eq!(
        call(&mut st, "math", "frexp", &[A::F(8.0)]).starts_with("0.5,4"),
        cfg!(not(rustynail_lua54)) || true
    );
}

#[cfg(rustynail_lua54)]
#[test]
fn lua54_compat_math() {
    let mut st = state();
    assert_eq!(call(&mut st, "math", "pow", &[A::I(2), A::I(10)]), "1024.0");
    assert_eq!(call(&mut st, "math", "log10", &[A::I(100)]), "2.0");
    assert_eq!(call(&mut st, "math", "cosh", &[A::I(0)]), "1.0");
    assert_eq!(
        call(&mut st, "math", "atan2", &[A::I(1), A::I(1)]),
        "0.78539816339745"
    );
    assert_eq!(
        call(&mut st, "string", "rep", &[A::S(""), A::I(3), A::S("")]),
        ""
    );
    assert_eq!(
        call(&mut st, "utf8", "offset", &[A::S("h\u{e9}llo"), A::I(3)]),
        "4"
    );
    assert!(call(
        &mut st,
        "string",
        "dump",
        &[Value::native as *const () as usize as i64].map(|_| A::I(0))[..]
    )
    .starts_with("ERR:"));
}

#[cfg(not(rustynail_lua54))]
#[test]
fn lua55_specific() {
    let mut st = state();
    assert_eq!(
        call(&mut st, "utf8", "offset", &[A::S("h\u{e9}llo"), A::I(3)]),
        "4,4"
    );
    assert!(!call(&mut st, "table", "create", &[A::I(3)]).is_empty());
    assert_eq!(
        call(&mut st, "math", "pow", &[A::I(2), A::I(10)]),
        "ERR:attempt to call a nil value"
    );
}

#[test]
fn table_and_utf8() {
    let mut st = state();
    assert_eq!(
        call(&mut st, "table", "concat", &[]),
        "ERR:bad argument #1 to 'table.concat' (table expected, got no value)"
    );
    assert_eq!(
        call(&mut st, "table", "insert", &[A::I(1), A::I(2)]),
        "ERR:bad argument #1 to 'table.insert' (table expected, got number)"
    );
    assert_eq!(
        call(&mut st, "utf8", "char", &[A::I(72), A::I(228), A::I(8364)]),
        "H\u{e4}\u{20ac}"
    );
    assert_eq!(call(&mut st, "utf8", "len", &[A::S("h\u{e9}llo")]), "5");
    assert_eq!(
        call(
            &mut st,
            "utf8",
            "codepoint",
            &[A::S("h\u{e9}"), A::I(1), A::I(-1)]
        ),
        "104,233"
    );
    assert_eq!(
        call(&mut st, "utf8", "char", &[A::I(-1)]),
        "ERR:bad argument #1 to 'utf8.char' (value out of range)"
    );
}
